//! rustty-vt page
//!
//! PORTS: ghostty src/terminal/page.zig
//!
//! PORT STATUS: stub
//!
//! NOTE ON "stub": per the sub-batch contract the crate-level lib.rs PORT STATUS
//! stays `stub` while leaves land; this module itself is fully ported.
//!
//! PORT NOTE: the reference imports are
//!   const std = @import("std");
//!   const builtin = @import("builtin");
//!   const build_options = @import("terminal_options");
//!   const Allocator = std.mem.Allocator;
//!   const ArenaAllocator = std.heap.ArenaAllocator;
//!   const assert = @import("../quirks.zig").inlineAssert;
//!   const testing = std.testing;
//!   const posix = std.posix;
//!   const windows = std.os.windows;
//!   const fastmem = @import("../fastmem.zig");
//!   const color = @import("color.zig");
//!   const hyperlink = @import("hyperlink.zig");
//!   const kitty = @import("kitty.zig");
//!   const stylepkg = @import("style.zig");
//!   const Style = stylepkg.Style;
//!   const StyleId = stylepkg.Id;
//!   const StyleSet = stylepkg.Set;
//!   const size = @import("size.zig");
//!   const getOffset = size.getOffset;
//!   const Offset = size.Offset;
//!   const OffsetBuf = size.OffsetBuf;
//!   const BitmapAllocator = @import("bitmap_allocator.zig").BitmapAllocator;
//!   const hash_map = @import("hash_map.zig");
//!   const AutoOffsetHashMap = hash_map.AutoOffsetHashMap;
//!   const alignForward = std.mem.alignForward;
//!   const alignBackward = std.mem.alignBackward;
//!
//! PORT NOTE: `Cell` and `Row` are reference `packed struct(u64)` types whose
//! exact bit layout is load-bearing: `Page` `@bitCast`s them, `@memset`s cell
//! arrays as `[]u64`, and stores them in mmap'd backing memory accessed through
//! relative `Offset`s. To reproduce the byte-identical packed layout (and stay
//! Miri-clean — no padding/uninit), both are modeled as `#[repr(transparent)]`
//! wrappers over the backing `u64` with bit-field accessors that match the Zig
//! least-significant-first packing exactly. `size_of`/`align_of`/bit-offset
//! tests assert the layout.
//!
//! PORT NOTE: `build_options.slow_runtime_safety` is modeled as
//! `cfg!(debug_assertions)` (the convention used by `ref_counted_set.rs`), and
//! `build_options.kitty_graphics` as the `KITTY_GRAPHICS` const (the reference
//! ghostty default is `true`; the page only uses it to mark the virtual
//! placeholder for U+10EEEE).

#![allow(clippy::missing_safety_doc)]
#![allow(non_camel_case_types)]
// PORT NOTE: the `Page` API mirrors the reference's `*Row`/`*Cell` raw-pointer
// signatures (the page is an offset-addressed arena; rows/cells are raw
// pointers into its mmap'd backing memory). Several methods therefore take raw
// pointers and dereference them; the safety contract is the same as the
// reference's (the pointers must come from this page). We allow the
// corresponding clippy lints module-wide rather than marking every accessor
// `unsafe` (which the reference does not, and which would not improve safety
// given the arena design).
#![allow(clippy::not_unsafe_ptr_arg_deref)]

use std::collections::HashMap;

// PORT NOTE: Rust has no `u21`. Zig's `u21` (used for codepoints) occupies the
// same 4-byte storage as `u32` (`@sizeOf(u21) == 4`), and codepoint values fit
// in 21 bits. We alias `u21 = u32` so codepoint-typed signatures read like the
// reference; the packed `Cell` storage masks to exactly 21/24 bits per the
// bit-layout tests, so no out-of-range bits leak beyond the field.
//
// Masking alone is NOT the whole story, in two places. (1) The packed `Cell`
// content union is 24 bits wide (sized to its widest member, RGB), so a
// codepoint write masked only to the field would persist bits 21..23 and build
// a cell the reference cannot represent — nonzero backing bits whose logical
// codepoint (low 21 bits) reads as zero, so `is_zero()` and `has_text()`
// disagree. (2) Grapheme TAILS never pass through a packed field at all: they
// are stored verbatim in the page's grapheme arena (`set_graphemes`/
// `append_grapheme`, read back by `lookup_grapheme`). In both cases the alias
// alone would let a direct Cell/Page-API caller persist a value outside the
// `u21` domain that the reference's type system rejects outright (a compile
// error for a constant, a safety-checked `@intCast` panic otherwise). Those
// entry points therefore `debug_assert!(cp <= U21_MAX)`, which is the port's
// standing map of a Zig safe-build check (Debug/ReleaseSafe trap, ReleaseFast
// unchecked).
#[allow(non_camel_case_types)]
pub type u21 = u32;

/// `std.math.maxInt(u21)` — the largest value the reference's `u21` codepoint
/// type can hold.
pub(crate) const U21_MAX: u21 = 0x1F_FFFF;

use rustty_render_types::term_size as size;
use rustty_render_types::term_size::{get_offset, Offset, OffsetBuf, OffsetSlice};
use rustty_util::fastmem;

use crate::bitmap_allocator::BitmapAllocator;
use crate::hash_map::{AutoContext, AutoOffsetHashMap};
use crate::hyperlink;
// PERF(port) (reshape): for the style-run `use_multiple` batch in the clone
// chain (see `clone_partial_row_from_inner`).
use crate::ref_counted_set::RefCountIntTrait;
use crate::style as stylepkg;
use crate::style::Id as StyleId;
use crate::style::Set as StyleSet;
use crate::style::Style;

// PORT NOTE: `build_options.kitty_graphics`. The reference enables the Kitty
// graphics protocol by default for the ghostty artifact; the page uses this only
// to set `Row.kitty_virtual_placeholder`. We keep the slot true so the layout
// and behavior match the reference's default build.
const KITTY_GRAPHICS: bool = true;

// PORT NOTE: `kitty.graphics.unicode.placeholder` is U+10EEEE. The kitty module
// surface is ported by a different sub-batch; we inline the constant here (the
// only use is the codepoint comparison below).
const KITTY_PLACEHOLDER: u21 = 0x10EEEE;

/// The allocator to use for multi-codepoint grapheme data. We use
/// a chunk size of 4 codepoints. It'd be best to set this empirically
/// but it is currently set based on vibes. My thinking around 4 codepoints
/// is that most skin-tone emoji are <= 4 codepoints, letter combiners
/// are usually <= 4 codepoints, and 4 codepoints is a nice power of two
/// for alignment.
const GRAPHEME_CHUNK_LEN: usize = 4;
const GRAPHEME_CHUNK: usize = GRAPHEME_CHUNK_LEN * core::mem::size_of::<u32>();
// pub const grapheme_max_len = 64;
pub const GRAPHEME_MAX_LEN: usize = 64;
// PORT NOTE: the reference uses `u21` for codepoints; Rust has no `u21`, so we
// use `u32` (which has the same 4-byte storage as Zig's `u21`, matching
// `@sizeOf(u21) == 4` for the grapheme allocation chunk math).
type GraphemeAlloc = BitmapAllocator<GRAPHEME_CHUNK>;
const GRAPHEME_COUNT_DEFAULT: usize = GraphemeAlloc::BITMAP_BIT_SIZE;
pub const GRAPHEME_BYTES_DEFAULT: usize = GRAPHEME_COUNT_DEFAULT * GRAPHEME_CHUNK;
type GraphemeMap = AutoOffsetHashMap<
    Offset<Cell>,
    OffsetSlice<u32>,
    { crate::hash_map::DEFAULT_MAX_LOAD_PERCENTAGE },
>;

/// The allocator used for shared utf8-encoded strings within a page.
/// Note the chunk size below is the minimum size of a single allocation
/// and requires a single bit of metadata in our bitmap allocator. Therefore
/// it should be tuned carefully (too small and we waste metadata, too large
/// and we have fragmentation). We can probably use a better allocation
/// strategy in the future.
///
/// At the time of writing this, the strings table is only used for OSC8
/// IDs and URIs. IDs are usually short and URIs are usually longer. I chose
/// 32 bytes as a compromise between these two since it represents single
/// domain links quite well and is not too wasteful for short IDs. We can
/// continue to tune this as we see how it's used.
const STRING_CHUNK_LEN: usize = 32;
const STRING_CHUNK: usize = STRING_CHUNK_LEN * core::mem::size_of::<u8>();
type StringAlloc = BitmapAllocator<STRING_CHUNK>;
const STRING_COUNT_DEFAULT: usize = StringAlloc::BITMAP_BIT_SIZE;
pub const STRING_BYTES_DEFAULT: usize = STRING_COUNT_DEFAULT * STRING_CHUNK;

/// Default number of hyperlinks we support.
///
/// The cell multiplier is the number of cells per hyperlink entry that
/// we support. A hyperlink can be longer than this multiplier; the multiplier
/// just sets the total capacity to simplify adjustable size metrics.
const HYPERLINK_COUNT_DEFAULT: usize = 4;
pub fn hyperlink_set_item_size() -> usize {
    core::mem::size_of::<
        crate::ref_counted_set::Item<hyperlink::PageEntry, hyperlink::Id, size::CellCountInt>,
    >()
}
fn hyperlink_bytes_default() -> usize {
    HYPERLINK_COUNT_DEFAULT * hyperlink_set_item_size()
}
const HYPERLINK_CELL_MULTIPLIER: usize = 16;

// PORT NOTE: `std.mem.alignForward(usize, v, a)` / `alignBackward`.
#[inline]
fn align_forward(v: usize, a: usize) -> usize {
    (v + a - 1) & !(a - 1)
}
#[inline]
fn align_backward(v: usize, a: usize) -> usize {
    v & !(a - 1)
}

// PORT NOTE: `std.heap.page_size_min`. In the reference this is a comptime
// constant selected per `(builtin.os.tag, builtin.cpu.arch)` from
// `std.heap.page_size_min_default` (lib/std/heap.zig). It is NOT always 4096:
// aarch64 Apple targets (macOS/iOS/...) use 16 KiB, WASI/emscripten use 64 KiB,
// some sparc64/m68k targets differ, etc. `Page.layout` aligns `total_size` up
// to this value (`alignForward(usize, hyperlink_map_end, page_size_min)`), so on
// Apple Silicon a non-standard (reduced-capacity) page rounds to a 16 KiB
// multiple, not a 4 KiB one. We therefore derive it from the build target to
// match the reference's comptime value rather than hardcoding 4096.
//
// The mmap fast-path allocates exactly `total_size` bytes and the OS-page
// alignment of an mmap result is always >= the kernel page size (16 KiB on
// aarch64-macos), so this layout value and the runtime allocation alignment
// agree on every target we build.
pub(crate) const PAGE_SIZE_MIN: usize = page_size_min_default();

/// Comptime `std.heap.page_size_min` for the build target, mirroring
/// `std.heap.page_size_min_default` in `lib/std/heap.zig`. We resolve it from
/// `cfg!(target_os/target_arch)` so it is a faithful per-target constant.
const fn page_size_min_default() -> usize {
    // macOS / iOS / tvOS / watchOS / visionOS / driverkit (Apple/Darwin):
    //   x86_64 => 4 KiB, aarch64 => 16 KiB.
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        16 << 10
    }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        4 << 10
    }
    #[cfg(all(target_os = "ios", target_arch = "aarch64"))]
    {
        16 << 10
    }
    // Linux: every arch we support uses 4 KiB (aarch64/x86_64/etc.). sparc64 is
    // 8 KiB in the reference but we do not target it.
    #[cfg(all(
        target_os = "linux",
        not(target_arch = "sparc64"),
        not(target_arch = "m68k")
    ))]
    {
        4 << 10
    }
    // Fallback for any other (os, arch) we have not enumerated: 4 KiB matches
    // the reference for all currently-supported x86_64/aarch64 Unix targets
    // (FreeBSD/NetBSD/OpenBSD/etc.). This keeps the const total even though we
    // only build/test on macOS and Linux.
    #[cfg(not(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "macos", target_arch = "x86_64"),
        all(target_os = "ios", target_arch = "aarch64"),
        all(
            target_os = "linux",
            not(target_arch = "sparc64"),
            not(target_arch = "m68k")
        )
    )))]
    {
        4 << 10
    }
}

// PORT NOTE: the reference selects `AllocWindows` or `AllocPosix` at comptime
// (`const PageAlloc = switch (builtin.os.tag) { .windows => AllocWindows, else
// => AllocPosix }`). Both arms are ported below, each behind the matching
// `cfg`; wasm32 is a third host with neither syscall (see its own PORT NOTE).
// The allocation must be page-aligned and zeroed; mmap(MAP_PRIVATE|
// MAP_ANONYMOUS) and VirtualAlloc(MEM_COMMIT|MEM_RESERVE) each guarantee both.
mod page_alloc {
    //! Page-aligned allocator used for terminal page backing memory. Pages
    //! require page-aligned, zeroed memory obtained directly from the OS
    //! (not the Zig allocator) because the allocation fast-path is
    //! performance-critical and the OS guarantees zeroed pages.
    //!
    //! Allocate page-aligned, zeroed backing memory using mmap with
    //! MAP_PRIVATE | MAP_ANONYMOUS which guarantees zeroed pages.
    //!
    //! Allocate page-aligned, zeroed backing memory using VirtualAlloc with
    //! MEM_COMMIT | MEM_RESERVE which guarantees zeroed pages.

    /// A page-aligned, zeroed mmap allocation. The pointer/len pair mirrors the
    /// reference's `[]align(page_size_min) u8`.
    pub struct Backing {
        pub ptr: *mut u8,
        pub len: usize,
    }

    /// `AllocPosix`: pub fn alloc(n: usize) ![]align(page_size_min) u8
    #[cfg(not(any(target_arch = "wasm32", windows)))]
    pub fn alloc(n: usize) -> Option<Backing> {
        // posix.mmap(null, n, PROT.READ|PROT.WRITE, .{ .TYPE=.PRIVATE, .ANONYMOUS=true }, -1, 0)
        let ptr = unsafe {
            libc::mmap(
                core::ptr::null_mut(),
                n,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
                -1,
                0,
            )
        };
        if ptr == libc::MAP_FAILED {
            return None;
        }
        Some(Backing {
            ptr: ptr as *mut u8,
            len: n,
        })
    }

    /// `AllocPosix`: pub fn free(mem: []align(page_size_min) u8) void
    #[cfg(not(any(target_arch = "wasm32", windows)))]
    pub fn free(b: &Backing) {
        unsafe {
            libc::munmap(b.ptr as *mut libc::c_void, b.len);
        }
    }

    // PORT NOTE (windows): `AllocWindows` (`src/terminal/page.zig:58-83`). The
    // reference reaches VirtualAlloc/VirtualFree as
    // `windows.exp.kernel32.VirtualAlloc` — i.e. THROUGH `os/windows.zig`, whose
    // port is `rustty_os::windows` (this crate already depends on rustty-os, so
    // the import adds no crate edge). `?LPVOID` optionals are Rust raw pointers,
    // which are already nullable, so `orelse return error.OutOfMemory` becomes
    // the null check below.

    /// `AllocWindows`: pub fn alloc(n: usize) error{OutOfMemory}![]align(page_size_min) u8
    #[cfg(windows)]
    pub fn alloc(n: usize) -> Option<Backing> {
        use rustty_os::windows;
        let addr = unsafe {
            windows::exp::kernel32::VirtualAlloc(
                core::ptr::null_mut(),
                n,
                windows::MEM_COMMIT | windows::MEM_RESERVE,
                windows::PAGE_READWRITE,
            )
        };
        if addr.is_null() {
            // orelse return error.OutOfMemory
            return None;
        }
        Some(Backing {
            ptr: addr as *mut u8,
            len: n,
        })
    }

    /// `AllocWindows`: pub fn free(mem: []align(page_size_min) u8) void
    #[cfg(windows)]
    pub fn free(b: &Backing) {
        use rustty_os::windows;
        unsafe {
            windows::exp::kernel32::VirtualFree(
                b.ptr as *mut core::ffi::c_void,
                0,
                windows::MEM_RELEASE,
            );
        }
    }

    // PORT NOTE: the reference calls `std.posix.mmap`/`std.posix.munmap` with
    // SYMBOLIC flags (`.{ .TYPE = .PRIVATE, .ANONYMOUS = true }`), so its
    // `AllocPosix` compiles on every POSIX target Zig supports. An earlier
    // revision of this port hand-rolled the mmap/munmap externs plus a
    // per-target `MAP_PRIVATE`/`MAP_ANONYMOUS` constant table to avoid a new
    // dependency; that table only had macOS and Linux arms, so the module (which
    // is compiled for every non-wasm target) failed to resolve those constants
    // on FreeBSD/NetBSD/OpenBSD/DragonFly/illumos. `libc` is already a direct
    // dependency of this crate (kitty/graphics_image's POSIX shm medium uses the
    // very same `libc::mmap`/`libc::MAP_FAILED`), so we take the flags from it:
    // that is the Rust analog of `std.posix`'s symbolic constants and is correct
    // on every POSIX target, exactly like the reference.

    // PORT NOTE (wasm32): the reference selects `AllocPosix` (mmap) or
    // `AllocWindows` (VirtualAlloc) at comptime. wasm32 is a THIRD host with no
    // `mmap` syscall, so we obtain the same page-aligned, zeroed backing the
    // module contract promises directly from the Rust global allocator via
    // `alloc_zeroed` with `page_size_min` alignment. This is the freestanding
    // analog of the wasm allocator ghostty's `os/wasm.zig` installs — no faked
    // behavior: the returned block is page-aligned and zeroed exactly as the
    // POSIX `MAP_PRIVATE|MAP_ANONYMOUS` path guarantees. `free` reconstructs the
    // identical `Layout` (same size + alignment) required by `dealloc`.
    #[cfg(target_arch = "wasm32")]
    fn backing_layout(n: usize) -> core::alloc::Layout {
        core::alloc::Layout::from_size_align(n, super::PAGE_SIZE_MIN)
            .expect("page backing layout is valid (n>0, align=page_size_min)")
    }

    /// pub fn alloc(n: usize) ![]align(page_size_min) u8
    #[cfg(target_arch = "wasm32")]
    pub fn alloc(n: usize) -> Option<Backing> {
        if n == 0 {
            return None;
        }
        // SAFETY: layout has non-zero size; `alloc_zeroed` yields page-aligned,
        // zeroed memory — the same contract as the POSIX mmap path.
        let ptr = unsafe { std::alloc::alloc_zeroed(backing_layout(n)) };
        if ptr.is_null() {
            return None;
        }
        Some(Backing { ptr, len: n })
    }

    /// pub fn free(mem: []align(page_size_min) u8) void
    #[cfg(target_arch = "wasm32")]
    pub fn free(b: &Backing) {
        // SAFETY: `b.ptr` came from `alloc_zeroed(backing_layout(b.len))`; the
        // reconstructed layout is identical, as `dealloc` requires.
        unsafe { std::alloc::dealloc(b.ptr, backing_layout(b.len)) }
    }
}

// PORT NOTE: the reference `Capacity` declares field DEFAULTS (`styles = 16`,
// `hyperlink_bytes = hyperlink_bytes_default`, `grapheme_bytes =
// grapheme_bytes_default`, `string_bytes = string_bytes_default`) so callers can
// write `.{ .cols = c, .rows = r }`. Every rustty call site so far spelled all
// six fields out, so the defaults had no Rust model; `Default` supplies them
// verbatim for `..Default::default()` struct-update callers (WS10 T11's
// HyperlinkMap benchmark is the first). `cols`/`rows` have no reference default;
// 0 is the only inert choice and callers always override them.
impl Default for Capacity {
    fn default() -> Capacity {
        Capacity {
            cols: 0,
            rows: 0,
            // styles: size.StyleCountInt = 16,
            styles: 16,
            // hyperlink_bytes: size.HyperlinkCountInt = hyperlink_bytes_default,
            hyperlink_bytes: hyperlink_bytes_default() as size::HyperlinkCountInt,
            // grapheme_bytes: size.GraphemeBytesInt = grapheme_bytes_default,
            grapheme_bytes: GRAPHEME_BYTES_DEFAULT as size::GraphemeBytesInt,
            // string_bytes: size.StringBytesInt = string_bytes_default,
            string_bytes: STRING_BYTES_DEFAULT as size::StringBytesInt,
        }
    }
}

/// The standard capacity for a page that doesn't have special
/// requirements. This is enough to support a very large number of cells.
/// The standard capacity is chosen as the fast-path for allocation since
/// pages of standard capacity use a pooled allocator instead of single-use
/// mmaps.
pub fn std_capacity() -> Capacity {
    Capacity {
        cols: 215,
        rows: 215,
        styles: 128,
        // grapheme_bytes = if (builtin.is_test) 512 else 8192
        grapheme_bytes: if cfg!(test) { 512 } else { 8192 },
        hyperlink_bytes: hyperlink_bytes_default() as size::HyperlinkCountInt,
        string_bytes: STRING_BYTES_DEFAULT as size::StringBytesInt,
    }
}

// ===========================================================================
// Cell / Row packed structs — byte-layout critical (see module PORT NOTE).
// ===========================================================================

/// `Cell.RGB = packed struct { r: u8, g: u8, b: u8 }`. Used in the cell content
/// union. 24 data bits.
// PORT NOTE: hoisted out of `Cell` (Rust nested type names). Matches the
// reference `Cell.RGB` field order/widths.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct CellRgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

/// `Cell.ContentTag = enum(u2)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContentTag {
    /// A single codepoint, could be zero to be empty cell.
    Codepoint = 0,

    /// A codepoint that is part of a multi-codepoint grapheme cluster.
    /// The codepoint tag is active in content, but also expect more
    /// codepoints in the grapheme data.
    CodepointGrapheme = 1,

    /// The cell has no text but only a background color. This is an
    /// optimization so that cells with only backgrounds don't take up
    /// style map space and also don't require a style map lookup.
    BgColorPalette = 2,
    BgColorRgb = 3,
}

impl ContentTag {
    #[inline]
    fn from_u2(v: u64) -> ContentTag {
        match v & 0b11 {
            0 => ContentTag::Codepoint,
            1 => ContentTag::CodepointGrapheme,
            2 => ContentTag::BgColorPalette,
            3 => ContentTag::BgColorRgb,
            _ => unreachable!(),
        }
    }
}

/// `Cell.Wide = enum(u2)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wide {
    /// Not a wide character, cell width 1.
    Narrow = 0,

    /// Wide character, cell width 2.
    Wide = 1,

    /// Spacer after wide character. Do not render.
    SpacerTail = 2,

    /// Spacer at the end of a soft-wrapped line to indicate that a wide
    /// character is continued on the next line.
    SpacerHead = 3,
}

impl Wide {
    #[inline]
    fn from_u2(v: u64) -> Wide {
        match v & 0b11 {
            0 => Wide::Narrow,
            1 => Wide::Wide,
            2 => Wide::SpacerTail,
            3 => Wide::SpacerHead,
            _ => unreachable!(),
        }
    }
}

/// `Cell.SemanticContent = enum(u2)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SemanticContent {
    /// Regular output content, such as command output.
    Output = 0,

    /// Content that is part of user input, such as the command
    /// to execute at a prompt.
    Input = 1,

    /// Content that is part of prompt emitted by the interactive
    /// application, such as "user@host >"
    Prompt = 2,
}

impl SemanticContent {
    #[inline]
    fn from_u2(v: u64) -> SemanticContent {
        match v & 0b11 {
            0 => SemanticContent::Output,
            1 => SemanticContent::Input,
            2 => SemanticContent::Prompt,
            // The reference enum only declares 0..2; value 3 is unused. The
            // zero-initialized backing memory never produces it, but we keep the
            // match total to avoid UB.
            _ => SemanticContent::Output,
        }
    }
}

/// A cell represents a single terminal grid cell.
///
/// The zero value of this struct must be a valid cell representing empty,
/// since we zero initialize the backing memory for a page.
///
/// PORT NOTE: reference `pub const Cell = packed struct(u64)`. Bit layout
/// (least-significant first, matching Zig packing):
///   bits  0..1   content_tag : ContentTag (u2)
///   bits  2..22  content     : union { codepoint:u21, color_palette:u8, color_rgb:RGB(u24) }  (21 bits wide)
///   bits 23..38  style_id    : StyleId (u16)
///   bits 39..40  wide        : Wide (u2)
///   bit  41      protected   : bool (u1)
///   bit  42      hyperlink   : bool (u1)
///   bits 43..44  semantic_content : SemanticContent (u2)
///   bits 45..60  _padding    : u16
/// Note: the union field is 21 bits wide overall (the widest member, u21);
/// `color_palette` occupies the low 8 bits and `color_rgb` the low 24... but the
/// union width is `@bitSizeOf` of the widest member = u21 = 21 bits. The
/// reference `Cell.RGB` is `packed struct { r,g,b: u8 }` = 24 bits, which is
/// WIDER than u21; Zig sizes the union to the max member, so the content union is
/// 24 bits. See the `comptime`/layout tests below which assert the total is u64.
#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct Cell(u64);

// Bit offsets/masks for Cell fields (see struct doc).
const CELL_CONTENT_TAG_SHIFT: u32 = 0;
const CELL_CONTENT_TAG_BITS: u32 = 2;
pub const CELL_CONTENT_SHIFT: u32 = 2;
pub const CELL_CONTENT_BITS: u32 = 24; // union sized to widest member RGB (u24)
pub const CELL_STYLE_ID_SHIFT: u32 = CELL_CONTENT_SHIFT + CELL_CONTENT_BITS; // 26
const CELL_STYLE_ID_BITS: u32 = 16;
const CELL_WIDE_SHIFT: u32 = CELL_STYLE_ID_SHIFT + CELL_STYLE_ID_BITS; // 42
const CELL_WIDE_BITS: u32 = 2;
const CELL_PROTECTED_SHIFT: u32 = CELL_WIDE_SHIFT + CELL_WIDE_BITS; // 44
const CELL_HYPERLINK_SHIFT: u32 = CELL_PROTECTED_SHIFT + 1; // 45
const CELL_SEMANTIC_CONTENT_SHIFT: u32 = CELL_HYPERLINK_SHIFT + 1; // 46
const CELL_SEMANTIC_CONTENT_BITS: u32 = 2;

// PORT NOTE (WS10 merge): `CellField` + `cell_field_mask` were ported
// INDEPENDENTLY by two WS10 lanes (T16 for print_slice, T19 for `Mask`) and
// collided here as duplicate definitions — git merged the text cleanly because
// they landed at different offsets. The T19 definition below is the one kept:
// `Mask` is generic over its output, so it needs `cell_field_mask -> Backing`.
// Both ports were faithful and variant-identical; only the helper methods
// differed (`shift()`/`bits()` vs `range()`).
// `_padding: u16` occupies bits 48..63. The accessors never touch it; these
// document the reference field's bit position and are asserted by the
// `cell_field_bit_offsets` layout test.
#[allow(dead_code)]
const CELL_PADDING_SHIFT: u32 = CELL_SEMANTIC_CONTENT_SHIFT + CELL_SEMANTIC_CONTENT_BITS; // 48
#[allow(dead_code)]
const CELL_PADDING_BITS: u32 = 16;

#[inline]
fn mask(bits: u32) -> u64 {
    if bits >= 64 {
        u64::MAX
    } else {
        (1u64 << bits) - 1
    }
}

#[inline]
fn get_bits(backing: u64, shift: u32, bits: u32) -> u64 {
    (backing >> shift) & mask(bits)
}

#[inline]
fn set_bits(backing: &mut u64, shift: u32, bits: u32, value: u64) {
    let m = mask(bits) << shift;
    *backing = (*backing & !m) | ((value << shift) & m);
}

impl Cell {
    /// A cell whose backing u64 is all zeros (the valid "empty" cell).
    #[inline]
    pub fn zeroed() -> Cell {
        Cell(0)
    }

    /// `@bitCast(@as(u64, x))`
    #[inline]
    pub fn from_bits(v: u64) -> Cell {
        Cell(v)
    }

    /// `@as(u64, @bitCast(self))`
    #[inline]
    pub fn to_bits(self) -> u64 {
        self.0
    }

    /// The content tag dictates the active tag in content and possibly
    /// some other behaviors.
    #[inline]
    pub fn content_tag(self) -> ContentTag {
        ContentTag::from_u2(get_bits(
            self.0,
            CELL_CONTENT_TAG_SHIFT,
            CELL_CONTENT_TAG_BITS,
        ))
    }
    #[inline]
    pub fn set_content_tag(&mut self, tag: ContentTag) {
        set_bits(
            &mut self.0,
            CELL_CONTENT_TAG_SHIFT,
            CELL_CONTENT_TAG_BITS,
            tag as u64,
        );
    }

    // The content of the cell. This is a union based on content_tag.
    //   content: packed union {
    //       /// The codepoint that this cell contains. If `grapheme` is false,
    //       /// then this is the only codepoint in the cell. If `grapheme` is
    //       /// true, then this is the first codepoint in the grapheme cluster.
    //       codepoint: u21,
    //
    //       /// The content is an empty cell with a background color.
    //       color_palette: u8,
    //       color_rgb: RGB,
    //   } = .{ .codepoint = 0 },

    /// `content.codepoint: u21`
    #[inline]
    pub fn content_codepoint(self) -> u21 {
        // Low 21 bits of the 24-bit content field.
        (get_bits(self.0, CELL_CONTENT_SHIFT, 21)) as u21
    }
    #[inline]
    pub fn set_content_codepoint(&mut self, cp: u21) {
        // PORT NOTE: the reference's parameter is typed `u21`, so a value above
        // `maxInt(u21)` is unrepresentable at the API boundary (a compile error
        // for a constant, a safety-checked `@intCast` panic otherwise). The
        // `u21 = u32` alias cannot enforce that, and the 24-bit union slot below
        // would happily persist bits 21..23 — producing states the reference
        // cannot build (e.g. `0x20_0000` stores nonzero backing bits while
        // `content_codepoint()`/`has_text()` read the low 21 bits as zero, so
        // `is_zero()` and `has_text()` disagree). Assert the domain instead,
        // matching the port's standing map of a Zig safe-build check
        // (Debug/ReleaseSafe trap, ReleaseFast unchecked).
        debug_assert!(cp <= U21_MAX, "codepoint out of u21 domain");
        // Writing the union overwrites the whole content field (24 bits): the
        // high 3 bits of the union storage become 0, matching Zig's behavior of
        // writing a u21 into the 24-bit union slot (upper bits zeroed by the
        // `init`/`@bitCast(0)` idiom; explicit field writes set only the active
        // member but the reference always zero-inits first).
        set_bits(
            &mut self.0,
            CELL_CONTENT_SHIFT,
            CELL_CONTENT_BITS,
            cp as u64,
        );
    }

    /// `content.color_palette: u8`
    #[inline]
    pub fn content_color_palette(self) -> u8 {
        get_bits(self.0, CELL_CONTENT_SHIFT, 8) as u8
    }
    #[inline]
    pub fn set_content_color_palette(&mut self, v: u8) {
        set_bits(&mut self.0, CELL_CONTENT_SHIFT, CELL_CONTENT_BITS, v as u64);
    }

    /// `content.color_rgb: RGB`
    #[inline]
    pub fn content_color_rgb(self) -> CellRgb {
        let raw = get_bits(self.0, CELL_CONTENT_SHIFT, 24);
        CellRgb {
            r: (raw & 0xFF) as u8,
            g: ((raw >> 8) & 0xFF) as u8,
            b: ((raw >> 16) & 0xFF) as u8,
        }
    }
    #[inline]
    pub fn set_content_color_rgb(&mut self, rgb: CellRgb) {
        let raw = (rgb.r as u64) | ((rgb.g as u64) << 8) | ((rgb.b as u64) << 16);
        set_bits(&mut self.0, CELL_CONTENT_SHIFT, CELL_CONTENT_BITS, raw);
    }

    /// The style ID to use for this cell within the style map. Zero
    /// is always the default style so no lookup is required.
    #[inline]
    pub fn style_id(self) -> StyleId {
        get_bits(self.0, CELL_STYLE_ID_SHIFT, CELL_STYLE_ID_BITS) as StyleId
    }
    #[inline]
    pub fn set_style_id(&mut self, id: StyleId) {
        set_bits(
            &mut self.0,
            CELL_STYLE_ID_SHIFT,
            CELL_STYLE_ID_BITS,
            id as u64,
        );
    }

    /// The wide property of this cell, for wide characters. Characters in
    /// a terminal grid can only be 1 or 2 cells wide. A wide character
    /// is always next to a spacer. This is used to determine both the width
    /// and spacer properties of a cell.
    #[inline]
    pub fn wide(self) -> Wide {
        Wide::from_u2(get_bits(self.0, CELL_WIDE_SHIFT, CELL_WIDE_BITS))
    }
    #[inline]
    pub fn set_wide(&mut self, w: Wide) {
        set_bits(&mut self.0, CELL_WIDE_SHIFT, CELL_WIDE_BITS, w as u64);
    }

    /// Whether this was written with the protection flag set.
    #[inline]
    pub fn protected(self) -> bool {
        get_bits(self.0, CELL_PROTECTED_SHIFT, 1) != 0
    }
    #[inline]
    pub fn set_protected(&mut self, v: bool) {
        set_bits(&mut self.0, CELL_PROTECTED_SHIFT, 1, v as u64);
    }

    /// Whether this cell is a hyperlink. If this is true then you must
    /// look up the hyperlink ID in the page hyperlink_map and the ID in
    /// the hyperlink_set to get the actual hyperlink data.
    #[inline]
    pub fn hyperlink(self) -> bool {
        get_bits(self.0, CELL_HYPERLINK_SHIFT, 1) != 0
    }
    #[inline]
    pub fn set_hyperlink(&mut self, v: bool) {
        set_bits(&mut self.0, CELL_HYPERLINK_SHIFT, 1, v as u64);
    }

    /// The semantic type of the content of this cell. This is used
    /// by the semantic prompt (OSC 133) set of sequences to understand
    /// boundary points for content.
    #[inline]
    pub fn semantic_content(self) -> SemanticContent {
        SemanticContent::from_u2(get_bits(
            self.0,
            CELL_SEMANTIC_CONTENT_SHIFT,
            CELL_SEMANTIC_CONTENT_BITS,
        ))
    }
    #[inline]
    pub fn set_semantic_content(&mut self, v: SemanticContent) {
        set_bits(
            &mut self.0,
            CELL_SEMANTIC_CONTENT_SHIFT,
            CELL_SEMANTIC_CONTENT_BITS,
            v as u64,
        );
    }

    /// C ABI type.
    // pub const C = u64;
    /// Returns this cell as a C ABI value.
    // PORT NOTE: `Cell.C = u64; pub fn cval(self) C`.
    #[inline]
    pub fn cval(self) -> u64 {
        self.0
    }

    /// Helper to make a cell that just has a codepoint.
    pub fn init(cp: u21) -> Cell {
        // We have to use this bitCast here to ensure that our memory is
        // zeroed. Otherwise, the content below will leave some uninitialized
        // memory in the packed union. Valgrind verifies this.
        let mut cell = Cell::from_bits(0);
        cell.set_content_tag(ContentTag::Codepoint);
        cell.set_content_codepoint(cp);
        cell
    }

    #[inline]
    pub fn is_zero(self) -> bool {
        self.0 == 0
    }

    /// Returns true if this cell represents a cell with text to render.
    ///
    /// Cases this returns false:
    ///   - Cell text is blank
    ///   - Cell is styled but only with a background color and no text
    ///   - Cell has a unicode placeholder for Kitty graphics protocol
    #[inline]
    pub fn has_text(self) -> bool {
        match self.content_tag() {
            ContentTag::Codepoint | ContentTag::CodepointGrapheme => self.content_codepoint() != 0,
            ContentTag::BgColorPalette | ContentTag::BgColorRgb => false,
        }
    }

    #[inline]
    pub fn codepoint(self) -> u21 {
        match self.content_tag() {
            ContentTag::Codepoint | ContentTag::CodepointGrapheme => self.content_codepoint(),
            ContentTag::BgColorPalette | ContentTag::BgColorRgb => 0,
        }
    }

    /// The width in grid cells that this cell takes up.
    #[inline]
    pub fn grid_width(self) -> u8 {
        match self.wide() {
            Wide::Narrow | Wide::SpacerHead | Wide::SpacerTail => 1,
            Wide::Wide => 2,
        }
    }

    #[inline]
    pub fn has_styling(self) -> bool {
        self.style_id() != stylepkg::DEFAULT_ID
    }

    /// Returns true if the cell has no text or styling.
    pub fn is_empty(self) -> bool {
        match self.content_tag() {
            // Textual cells are empty if they have no text and are narrow.
            // The "narrow" requirement is because wide spacers are meaningful.
            ContentTag::Codepoint | ContentTag::CodepointGrapheme => {
                !self.has_text() && self.wide() == Wide::Narrow
            }
            ContentTag::BgColorPalette | ContentTag::BgColorRgb => false,
        }
    }

    #[inline]
    pub fn has_grapheme(self) -> bool {
        self.content_tag() == ContentTag::CodepointGrapheme
    }

    /// Copy of this cell with the managed-memory markers reset.
    // PERF(port): masked-store form of the three sequential resets in
    // `clonePartialRowFrom` (page.zig:905-909):
    //   dst_cell.hyperlink = false;
    //   dst_cell.style_id = stylepkg.default_id;
    //   if (dst_cell.content_tag == .codepoint_grapheme) {
    //       dst_cell.content_tag = .codepoint;
    //   }
    // Zig's packed-struct writes fold to exactly this under ReleaseFast; the
    // Rust bitfield setters were 3 separate read-modify-write stores. Note
    // `stylepkg::DEFAULT_ID == 0` so clearing the style_id bits IS the reset,
    // and `CodepointGrapheme == 1` -> `Codepoint == 0`, so clearing the 2 tag
    // bits IS the retag. Machine-checked against the sequential setter form
    // by `cell_copy_with_managed_reset_equivalence`.
    #[inline(always)]
    pub(crate) fn copy_with_managed_reset(self) -> Cell {
        let mut v = self.0;
        v &= !((1u64 << CELL_HYPERLINK_SHIFT) | (mask(CELL_STYLE_ID_BITS) << CELL_STYLE_ID_SHIFT));
        if v & mask(CELL_CONTENT_TAG_BITS) == ContentTag::CodepointGrapheme as u64 {
            v &= !mask(CELL_CONTENT_TAG_BITS);
        }
        Cell(v)
    }

    /// True if, in the managed-copy loop of `clonePartialRowFrom`
    /// (page.zig:895-984), the grapheme and hyperlink arms are both dormant
    /// for this source cell — i.e. the only per-cell work is the reset copy
    /// plus (possibly) the style arm and the kitty row flag.
    // PERF(port) (reshape): guards the style-run bulk path on the
    // different-page clone: for such a cell,
    // `copy_with_managed_reset` reduces to "clear the style_id field"
    // (the hyperlink bit is already 0 and the tag is not
    // codepoint_grapheme), so a run of consecutive such cells sharing one
    // style_id can be copied wholesale and its style refcounts bumped in one
    // `use_multiple`. Machine-checked by `cell_style_run_facts`.
    #[inline(always)]
    pub(crate) fn is_style_run_cell(self) -> bool {
        let v = self.0;
        (v & (1u64 << CELL_HYPERLINK_SHIFT)) == 0
            && (v & mask(CELL_CONTENT_TAG_BITS)) != ContentTag::CodepointGrapheme as u64
    }

    /// Returns true if the set of cells has text in it.
    #[inline]
    pub fn has_text_any(cells: &[Cell]) -> bool {
        for cell in cells {
            if cell.has_text() {
                return true;
            }
        }
        false
    }
}

impl Default for Cell {
    fn default() -> Self {
        Cell::zeroed()
    }
}

/// The semantic prompt state of the row. See `semantic_prompt`.
// PORT NOTE: `Row.SemanticPrompt = enum(u2)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SemanticPrompt {
    /// No prompt cells in this row.
    None = 0,
    /// Prompt cells exist in this row and this is a primary prompt
    /// line. A primary prompt line is one that is not a continuation
    /// and is the beginning of a prompt.
    Prompt = 1,
    /// Prompt cells exist in this row that had k=c set (continuation)
    /// line. This is used as a way to detect when a line should
    /// be considered part of some prior prompt. If no prior prompt
    /// is found, the last (most historical) prompt continuation line is
    /// considered the prompt.
    PromptContinuation = 2,
}

impl SemanticPrompt {
    #[inline]
    fn from_u2(v: u64) -> SemanticPrompt {
        match v & 0b11 {
            0 => SemanticPrompt::None,
            1 => SemanticPrompt::Prompt,
            2 => SemanticPrompt::PromptContinuation,
            _ => SemanticPrompt::None,
        }
    }
}

/// `pub const Row = packed struct(u64)`. Bit layout (least-significant first):
///   bits  0..31  cells: Offset(Cell) (OffsetInt == u32)
///   bit  32      wrap : bool
///   bit  33      wrap_continuation : bool
///   bit  34      grapheme : bool
///   bit  35      styled : bool
///   bit  36      hyperlink : bool
///   bits 37..38  semantic_prompt : SemanticPrompt (u2)
///   bit  39      kitty_virtual_placeholder : bool
///   bit  40      dirty : bool
///   bits 41..63  _padding : u23
#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct Row(u64);

const ROW_CELLS_SHIFT: u32 = 0;
const ROW_CELLS_BITS: u32 = 32;
const ROW_WRAP_SHIFT: u32 = 32;
const ROW_WRAP_CONTINUATION_SHIFT: u32 = 33;
const ROW_GRAPHEME_SHIFT: u32 = 34;
const ROW_STYLED_SHIFT: u32 = 35;
const ROW_HYPERLINK_SHIFT: u32 = 36;
const ROW_SEMANTIC_PROMPT_SHIFT: u32 = 37;
const ROW_SEMANTIC_PROMPT_BITS: u32 = 2;
const ROW_KITTY_PLACEHOLDER_SHIFT: u32 = 39;
const ROW_DIRTY_SHIFT: u32 = 40;
// `_padding: u23` occupies bits 41..63. The accessors never touch it; these
// document the reference field's bit position and are asserted by the
// `row_field_bit_offsets` layout test.
#[allow(dead_code)]
const ROW_PADDING_SHIFT: u32 = 41;
#[allow(dead_code)]
const ROW_PADDING_BITS: u32 = 23;

impl Row {
    #[inline]
    pub fn zeroed() -> Row {
        Row(0)
    }
    #[inline]
    pub fn from_bits(v: u64) -> Row {
        Row(v)
    }
    #[inline]
    pub fn to_bits(self) -> u64 {
        self.0
    }

    /// The cells in the row offset from the page.
    #[inline]
    pub fn cells(self) -> Offset<Cell> {
        Offset::new(get_bits(self.0, ROW_CELLS_SHIFT, ROW_CELLS_BITS) as size::OffsetInt)
    }
    #[inline]
    pub fn set_cells(&mut self, off: Offset<Cell>) {
        set_bits(
            &mut self.0,
            ROW_CELLS_SHIFT,
            ROW_CELLS_BITS,
            off.offset as u64,
        );
    }

    /// True if this row is soft-wrapped. The first cell of the next
    /// row is a continuation of this row.
    #[inline]
    pub fn wrap(self) -> bool {
        get_bits(self.0, ROW_WRAP_SHIFT, 1) != 0
    }
    #[inline]
    pub fn set_wrap(&mut self, v: bool) {
        set_bits(&mut self.0, ROW_WRAP_SHIFT, 1, v as u64);
    }

    /// True if the previous row to this one is soft-wrapped and
    /// this row is a continuation of that row.
    #[inline]
    pub fn wrap_continuation(self) -> bool {
        get_bits(self.0, ROW_WRAP_CONTINUATION_SHIFT, 1) != 0
    }
    #[inline]
    pub fn set_wrap_continuation(&mut self, v: bool) {
        set_bits(&mut self.0, ROW_WRAP_CONTINUATION_SHIFT, 1, v as u64);
    }

    /// True if any of the cells in this row have multi-codepoint
    /// grapheme clusters. If this is true, some fast paths are not
    /// possible because erasing for example may need to clear existing
    /// grapheme data.
    #[inline]
    pub fn grapheme(self) -> bool {
        get_bits(self.0, ROW_GRAPHEME_SHIFT, 1) != 0
    }
    #[inline]
    pub fn set_grapheme(&mut self, v: bool) {
        set_bits(&mut self.0, ROW_GRAPHEME_SHIFT, 1, v as u64);
    }

    /// True if any of the cells in this row have a ref-counted style.
    /// This can have false positives but never a false negative. Meaning:
    /// this will be set to true the first time a style is used, but it
    /// will not be set to false if the style is no longer used, because
    /// checking for that condition is too expensive.
    ///
    /// Why have this weird false positive flag at all? This makes VT operations
    /// that erase cells (such as insert lines, delete lines, erase chars,
    /// etc.) MUCH MUCH faster in the case that the row was never styled.
    /// At the time of writing this, the speed difference is around 4x.
    #[inline]
    pub fn styled(self) -> bool {
        get_bits(self.0, ROW_STYLED_SHIFT, 1) != 0
    }
    #[inline]
    pub fn set_styled(&mut self, v: bool) {
        set_bits(&mut self.0, ROW_STYLED_SHIFT, 1, v as u64);
    }

    /// True if any of the cells in this row are part of a hyperlink.
    /// This is similar to styled: it can have false positives but never
    /// false negatives. This is used to optimize hyperlink operations.
    #[inline]
    pub fn hyperlink(self) -> bool {
        get_bits(self.0, ROW_HYPERLINK_SHIFT, 1) != 0
    }
    #[inline]
    pub fn set_hyperlink(&mut self, v: bool) {
        set_bits(&mut self.0, ROW_HYPERLINK_SHIFT, 1, v as u64);
    }

    /// The semantic prompt state for this row.
    ///
    /// This is ONLY meant to note if there are ANY cells in this
    /// row that are part of a prompt. This is an optimization for more
    /// efficiently implementing jump-to-prompt operations.
    ///
    /// This may contain false positives but never false negatives. If
    /// this is set, you should still check individual cells to see if they
    /// have prompt semantics.
    #[inline]
    pub fn semantic_prompt(self) -> SemanticPrompt {
        SemanticPrompt::from_u2(get_bits(
            self.0,
            ROW_SEMANTIC_PROMPT_SHIFT,
            ROW_SEMANTIC_PROMPT_BITS,
        ))
    }
    #[inline]
    pub fn set_semantic_prompt(&mut self, v: SemanticPrompt) {
        set_bits(
            &mut self.0,
            ROW_SEMANTIC_PROMPT_SHIFT,
            ROW_SEMANTIC_PROMPT_BITS,
            v as u64,
        );
    }

    /// True if this row contains a virtual placeholder for the Kitty
    /// graphics protocol. (U+10EEEE)
    // Note: We keep this as memory-using even if the kitty graphics
    // feature is disabled because we want to keep our padding and
    // everything throughout the same.
    #[inline]
    pub fn kitty_virtual_placeholder(self) -> bool {
        get_bits(self.0, ROW_KITTY_PLACEHOLDER_SHIFT, 1) != 0
    }
    #[inline]
    pub fn set_kitty_virtual_placeholder(&mut self, v: bool) {
        set_bits(&mut self.0, ROW_KITTY_PLACEHOLDER_SHIFT, 1, v as u64);
    }

    /// True if this row is dirty and requires a redraw. This is set to true
    /// by any operation that modifies the row's contents or position, and
    /// consumers of the page are expected to clear it when they redraw.
    ///
    /// Dirty status is only ever meant to convey that one or more cells in
    /// the row have changed visually. A cell which changes in a way that
    /// doesn't affect the visual representation may not be marked as dirty.
    ///
    /// Dirty tracking may have false positives but should never have false
    /// negatives. A false negative would result in a visual artifact on the
    /// screen.
    #[inline]
    pub fn dirty(self) -> bool {
        get_bits(self.0, ROW_DIRTY_SHIFT, 1) != 0
    }
    #[inline]
    pub fn set_dirty(&mut self, v: bool) {
        set_bits(&mut self.0, ROW_DIRTY_SHIFT, 1, v as u64);
    }

    /// C ABI type.
    // pub const C = u64;
    /// Returns this row as a C ABI value.
    // PORT NOTE: `Row.C = u64; pub fn cval(self) C`.
    #[inline]
    pub fn cval(self) -> u64 {
        self.0
    }

    /// Returns true if this row has any managed memory outside of the
    /// row structure (graphemes, styles, etc.)
    #[inline]
    pub fn managed_memory(self) -> bool {
        // Ordered on purpose for likelihood.
        self.styled() || self.hyperlink() || self.grapheme()
    }
}

impl Default for Row {
    fn default() -> Self {
        Row::zeroed()
    }
}

// The PageList memory pool requires that initBuf overwrites at
// least the first pointer-size bytes of the backing buffer:
// std.heap.MemoryPool stores its free list node there when a
// page buffer is returned to it, and pool reuse skips zeroing
// in release builds. This holds because the rows array is at
// offset 0 (see layout), a page always has at least one row,
// and initBuf fully rewrites every row.
//
// PORT NOTE (oracle c442eb4b3, `P3-W1 T4`): the reference puts this in the
// `comptime { ... }` block at the top of `Page`; rustty's `Page` lives in
// `page_impl.rs` while `Row` is defined here, so the `const _` assert is
// anchored to `Row` and cross-referenced from both `init_buf` and `layout`.
// This is NOT decorative in rustty: `decommit_pool_item`
// (`pagelist/mod.rs`) is live, so rustty already depends on this invariant,
// and rustty skips the zeroing memset in release exactly as Zig does. A
// layout reorder that moved `rows` off offset 0, or a zero-row page, would
// corrupt pages in RELEASE BUILDS ONLY and non-deterministically, depending
// on pool reuse.
const _: () = assert!(core::mem::size_of::<Row>() >= core::mem::size_of::<usize>());

/// The size of this page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageSize {
    pub cols: size::CellCountInt,
    pub rows: size::CellCountInt,
}

/// Capacity of this page.
///
/// This capacity can be maxed out (every field max) and still fit
/// within a 64-bit memory space. If you need more than this, you will
/// need to split data across separate pages.
///
/// For 32-bit systems, it is possible to overflow the addressable
/// space and this is something we still need to address in the future
/// likely by limiting the maximum capacity on 32-bit systems further.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Capacity {
    /// Number of columns and rows we can know about.
    pub cols: size::CellCountInt,
    pub rows: size::CellCountInt,

    /// Number of unique styles that can be used on this page.
    pub styles: size::StyleCountInt,

    /// Number of bytes to allocate for hyperlink data. Note that the
    /// amount of data used for hyperlinks in total is more than this because
    /// hyperlinks use string data as well as a small amount of lookup metadata.
    /// This number is a rough approximation.
    pub hyperlink_bytes: size::HyperlinkCountInt,

    /// Number of bytes to allocate for grapheme data.
    pub grapheme_bytes: size::GraphemeBytesInt,

    /// Number of bytes to allocate for strings.
    pub string_bytes: size::StringBytesInt,
}

/// `Capacity.Adjustment`
#[derive(Clone, Copy, Debug, Default)]
pub struct Adjustment {
    pub cols: Option<size::CellCountInt>,
}

/// The allocation-failure error mirroring Zig's `Allocator.Error` / OutOfMemory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OutOfMemory;

impl Capacity {
    // PORT NOTE: the reference field defaults are applied via a builder-style
    // constructor since Rust structs can't have per-field defaults. `cap_new`
    // mirrors `Capacity{ .cols = c, .rows = r }` with the reference defaults:
    //   styles = 16, hyperlink_bytes = hyperlink_bytes_default,
    //   grapheme_bytes = grapheme_bytes_default, string_bytes = string_bytes_default.
    pub fn new(cols: size::CellCountInt, rows: size::CellCountInt) -> Capacity {
        Capacity {
            cols,
            rows,
            styles: 16,
            hyperlink_bytes: hyperlink_bytes_default() as size::HyperlinkCountInt,
            grapheme_bytes: GRAPHEME_BYTES_DEFAULT as size::GraphemeBytesInt,
            string_bytes: STRING_BYTES_DEFAULT as size::StringBytesInt,
        }
    }

    /// Returns the maximum number of columns that can be used with this
    /// capacity while still fitting at least one row. Returns null if even
    /// a single column cannot fit (which would indicate an unusable capacity).
    ///
    /// Note that this is the maximum number of columns that never increases
    /// the amount of memory the original capacity will take. If you modify
    /// the original capacity to add rows, then you can fit more columns.
    pub fn max_cols(self) -> Option<size::CellCountInt> {
        let available_bits = self.available_bits_for_grid();

        // If we can't even fit the row metadata, return null
        if available_bits <= bit_size_of_row() {
            return None;
        }

        // We do the math of how many columns we can fit in the remaining
        // bits ignoring the metadata of a row.
        let remaining_bits = available_bits - bit_size_of_row();
        let max_cols = remaining_bits / bit_size_of_cell();

        // Clamp to CellCountInt max
        Some(core::cmp::min(size::CellCountInt::MAX as usize, max_cols) as size::CellCountInt)
    }

    /// Adjust the capacity parameters while retaining the same total size.
    ///
    /// Adjustments always happen by limiting the rows in the page. Everything
    /// else can grow. If it is impossible to achieve the desired adjustment,
    /// OutOfMemory is returned.
    pub fn adjust(self, req: Adjustment) -> Result<Capacity, OutOfMemory> {
        let mut adjusted = self;
        if let Some(cols) = req.cols {
            let available_bits = self.available_bits_for_grid();

            // The size per row is:
            //   - The row metadata itself
            //   - The cells per row (n=cols)
            let bits_per_row: usize = bit_size_of_row() + bit_size_of_cell() * (cols as usize);
            let new_rows: usize = available_bits / bits_per_row;

            // If our rows go to zero then we can't fit any row metadata
            // for the desired number of columns.
            if new_rows == 0 {
                return Err(OutOfMemory);
            }

            adjusted.cols = cols;
            adjusted.rows = new_rows as size::CellCountInt;
        }

        Ok(adjusted)
    }

    /// Computes the number of bits available for rows and cells in the page.
    ///
    /// This is done by laying out the "meta" members (styles, graphemes,
    /// hyperlinks, strings) from the end of the page and finding where they
    /// start, which gives us the space available for rows and cells.
    fn available_bits_for_grid(self) -> usize {
        // The math below only works if there is no alignment gap between
        // the end of the rows array and the start of the cells array.
        //
        // To guarantee this, we assert that Row's size is a multiple of
        // Cell's alignment, so that any length array of Rows will end on
        // a valid alignment for the start of the Cell array.
        assert!(core::mem::size_of::<Row>().is_multiple_of(core::mem::align_of::<Cell>()));

        let l = Page::layout(self);

        // Layout meta members from the end to find styles_start
        let hyperlink_map_start = align_backward(
            l.total_size - l.hyperlink_map_layout.total_size,
            hyperlink_map_base_align(),
        );
        let hyperlink_set_start = align_backward(
            hyperlink_map_start - l.hyperlink_set_layout.total_size,
            hyperlink_set_base_align(),
        );
        let string_alloc_start = align_backward(
            hyperlink_set_start - l.string_alloc_layout.total_size,
            StringAlloc::BASE_ALIGN,
        );
        let grapheme_map_start = align_backward(
            string_alloc_start - l.grapheme_map_layout.total_size,
            grapheme_map_base_align(),
        );
        let grapheme_alloc_start = align_backward(
            grapheme_map_start - l.grapheme_alloc_layout.total_size,
            GraphemeAlloc::BASE_ALIGN,
        );
        let styles_start = align_backward(
            grapheme_alloc_start - l.styles_layout.total_size,
            style_set_base_align(),
        );

        // Multiply by 8 to convert bytes to bits
        styles_start * 8
    }
}

// PORT NOTE (`0e8b7bea6` "vt: expose packed cell layout" — CORE half,
// P4-W2 T4): the reference adds `Cell.CLayout = lib.Packed(Cell, .{ .fields =
// ... })` here — C-facing layout METADATA (reflected bit offsets/widths for
// every field, including the tagged-union content arms `codepoint` /
// `codepoint_grapheme` / `bg_color_palette` / `bg_color_rgb`, with
// `_pad`/`_padding` omitted) kept next to Cell itself and consumed by the ABI
// manifest (`c/types.zig`, `types.schema.json`). It is pure comptime
// reflection over `lib.Packed`/`lib.PackedTaggedUnion` (`src/lib/packed.zig`,
// +290) which rustty-lib does not carry yet: that machinery is area C's W5
// Bundle T ("packed.zig GhosttyPackedCell (couples W2's cell layout)"), and
// the Rust `CLayout` equivalent lands WITH it against this anchor. The
// physical bit ranges it would reflect are already named here by
// `CellField::range` below (single source for shifts/widths).
//
// =============================================================================
// Masked compares over the packed backing integers (446f80f4e, WS10 T19).
// =============================================================================

/// The backing integer of this packed struct. Prefer this over
/// hardcoding the integer type so that code is resilient to the
/// size changing.
///
/// PORT NOTE: the reference exposes this per type as
/// `pub const Backing = @typeInfo(Row).@"struct".backing_integer.?`.
/// Rust has no such reflection over a
/// hand-rolled bitfield, so the port names the type once here and both `Row` and
/// `Cell` alias it as `Row::Backing`/`Cell::Backing` (the layout tests in
/// `page_tests.rs` already assert both are 64 bits wide).
pub type Backing = u64;

/// A field of `Cell`, naming the bit range the masks operate on.
///
/// PORT NOTE: the reference identifies fields by comptime STRING
/// (`fieldMask(Cell, &.{"content_tag", "style_id"})`) and resolves them with
/// `@bitOffsetOf`/`@FieldType`. Rust cannot reflect over the port's hand-rolled
/// bitfield accessors, so fields are named by this enum instead — which is
/// strictly stronger than the reference here, because a typo is a compile error
/// rather than a comptime lookup failure at the use site.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CellField {
    ContentTag,
    Content,
    /// The `data: u21` bits of the `content.codepoint` union member — the
    /// reference's nested field path `"content.codepoint.data"`
    /// (`0fb3565c7`). Packed union members all share bit offset zero, so the
    /// path's bit offset is the content field's own offset and its width is
    /// the codepoint's 21 bits, excluding the `_pad: u3` next to it.
    ContentCodepointData,
    StyleId,
    Wide,
    Protected,
    Hyperlink,
    SemanticContent,
}

impl CellField {
    /// `(shift, bits)` of this field in `Cell`'s backing integer.
    pub const fn range(self) -> (u32, u32) {
        match self {
            CellField::ContentTag => (CELL_CONTENT_TAG_SHIFT, CELL_CONTENT_TAG_BITS),
            CellField::Content => (CELL_CONTENT_SHIFT, CELL_CONTENT_BITS),
            // `@bitSizeOf(u21)` bits at the content union's bit offset.
            CellField::ContentCodepointData => (CELL_CONTENT_SHIFT, 21),
            CellField::StyleId => (CELL_STYLE_ID_SHIFT, CELL_STYLE_ID_BITS),
            CellField::Wide => (CELL_WIDE_SHIFT, CELL_WIDE_BITS),
            CellField::Protected => (CELL_PROTECTED_SHIFT, 1),
            CellField::Hyperlink => (CELL_HYPERLINK_SHIFT, 1),
            CellField::SemanticContent => (CELL_SEMANTIC_CONTENT_SHIFT, CELL_SEMANTIC_CONTENT_BITS),
        }
    }
}

/// A field of `Row`. See `CellField`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RowField {
    Cells,
    Wrap,
    WrapContinuation,
    Grapheme,
    Styled,
    Hyperlink,
    SemanticPrompt,
    KittyVirtualPlaceholder,
    Dirty,
}

impl RowField {
    /// `(shift, bits)` of this field in `Row`'s backing integer.
    pub const fn range(self) -> (u32, u32) {
        match self {
            RowField::Cells => (ROW_CELLS_SHIFT, ROW_CELLS_BITS),
            RowField::Wrap => (ROW_WRAP_SHIFT, 1),
            RowField::WrapContinuation => (ROW_WRAP_CONTINUATION_SHIFT, 1),
            RowField::Grapheme => (ROW_GRAPHEME_SHIFT, 1),
            RowField::Styled => (ROW_STYLED_SHIFT, 1),
            RowField::Hyperlink => (ROW_HYPERLINK_SHIFT, 1),
            RowField::SemanticPrompt => (ROW_SEMANTIC_PROMPT_SHIFT, ROW_SEMANTIC_PROMPT_BITS),
            RowField::KittyVirtualPlaceholder => (ROW_KITTY_PLACEHOLDER_SHIFT, 1),
            RowField::Dirty => (ROW_DIRTY_SHIFT, 1),
        }
    }
}

/// Returns a mask with all bits set for the given fields of the packed
/// struct T, used for masked compares of raw backing-integer values
/// (e.g. `Row.Backing`, `Cell.Backing`). This is an implementation
/// detail of `Mask`, which is the public API built on top of this.
///
/// A field may be a dot-separated path (e.g. "content.codepoint.data")
/// to cover only a nested field of a packed struct or packed union
/// member. This allows a mask to be more precise than a whole
/// top-level field, e.g. covering the codepoint bits of a cell without
/// its padding.
///
/// PORT NOTE: reference `fn fieldMask(comptime T, comptime fields)`; the port
/// takes the already-resolved `(shift, bits)` field ranges. Nested paths
/// (`0fb3565c7`) become dedicated enum variants carrying their resolved range
/// (see `CellField::ContentCodepointData` — packed union members all share
/// bit offset zero, so the resolution is a constant).
pub const fn field_mask(fields: &[(u32, u32)]) -> Backing {
    let mut m: Backing = 0;
    let mut i = 0;
    while i < fields.len() {
        let (shift, bits) = fields[i];
        // `mask(bits) << shift`, spelled out because `mask` is not a const fn.
        let ones: Backing = if bits >= 64 {
            Backing::MAX
        } else {
            (1 << bits) - 1
        };
        m |= ones << shift;
        i += 1;
    }
    m
}

/// `field_mask` over `CellField`s.
pub const fn cell_field_mask(fields: &[CellField]) -> Backing {
    let mut m: Backing = 0;
    let mut i = 0;
    while i < fields.len() {
        let (shift, bits) = fields[i].range();
        let ones: Backing = if bits >= 64 {
            Backing::MAX
        } else {
            (1 << bits) - 1
        };
        m |= ones << shift;
        i += 1;
    }
    m
}

/// `field_mask` over `RowField`s.
pub const fn row_field_mask(fields: &[RowField]) -> Backing {
    let mut m: Backing = 0;
    let mut i = 0;
    while i < fields.len() {
        let (shift, bits) = fields[i].range();
        let ones: Backing = if bits >= 64 {
            Backing::MAX
        } else {
            (1 << bits) - 1
        };
        m |= ones << shift;
        i += 1;
    }
    m
}

/// A value whose whole representation is one `Backing` integer — `Row`, `Cell`.
///
/// PORT NOTE: `Mask` is generic over `comptime T: type` in the reference and
/// gets `@bitCast` for free. In Rust the bit-cast has to be a trait, and it is
/// the *transparency* of the `#[repr(transparent)] struct X(u64)` port shape
/// that makes it sound: `to_bits`/`from_bits` are the identity.
pub trait Packed: Copy {
    fn to_bits(self) -> Backing;
    fn from_bits(v: Backing) -> Self;
}

impl Packed for Cell {
    #[inline]
    fn to_bits(self) -> Backing {
        Cell::to_bits(self)
    }
    #[inline]
    fn from_bits(v: Backing) -> Self {
        Cell::from_bits(v)
    }
}

impl Packed for Row {
    #[inline]
    fn to_bits(self) -> Backing {
        Row::to_bits(self)
    }
    #[inline]
    fn from_bits(v: Backing) -> Self {
        Row::from_bits(v)
    }
}

/// A comptime-generated helper for classifying and comparing packed
/// struct values (e.g. Row, Cell) in bulk, using masked compares of
/// their raw backing integers.
///
/// Masked compares are the key to making bulk row/cell processing fast.
/// Rows and cells are small packed structs specifically so that a single
/// integer load observes every field at once. A masked compare can then
/// answer a multi-field question with one AND and one compare, instead
/// of extracting and branching on each field individually (each packed
/// field access compiles to its own shift/mask). Just as importantly,
/// the integer form vectorizes trivially: `@splat` the mask and expected
/// value, and whole groups of rows or cells can be classified with a
/// few SIMD instructions.
///
/// Some real examples of this in use:
///
///   - Terminal print fast path: a cell can be overwritten by the
///     simple/fast path only if its content tag is a plain codepoint,
///     it has no style, isn't wide, and isn't a hyperlink. Masking
///     with those fields and comparing against a template answers all
///     four questions in one compare per cell.
///
///   - Render state updates: a cell whose masked
///     `{content_tag, style_id}` bits are zero is a plain cell that
///     needs no managed-memory handling, so a vector OR-reduce can
///     skip entire groups of plain cells at once. Similarly, cells
///     whose masked bits equal the first cell's form a run sharing one
///     style, letting the update record one style lookup per run
///     rather than per cell.
///
///   - Dirty scans: OR-reducing groups of rows against the `dirty`
///     field mask finds whether any row in the group needs a rebuild
///     without touching each row's flag individually.
///
/// T is the packed struct type, fields are the fields covered by the
/// mask, and group_len is the number of values processed at once by
/// the group (vectorized) operations. Callers typically scan a slice
/// with the group operations and fall back to the scalar variants for
/// the remainder and for pinpointing values within a matched group.
///
/// In this port `MASK` is the already-reduced field-bit set (build it with
/// `cell_field_mask`/`row_field_mask`) and `GROUP_LEN` is `group_len`.
///
/// PORT NOTE: the reference is `pub fn Mask(comptime T, comptime fields,
/// comptime group_len) type` returning an anonymous struct; the Rust port is a
/// ZST generic over the same three parameters, with the field list already
/// reduced to its mask (Rust const generics take values, not slices). The
/// reference's `@Vector(group_len, Backing)` becomes `[Backing; GROUP_LEN]` —
/// LLVM auto-vectorizes fixed-size array ANDs/compares the same way, and unlike
/// `@Vector` it needs no target-feature gate.
///
/// A `group_len` of zero is rejected exactly where the reference rejects it:
/// naming the type is fine, but instantiating any group operation is a compile
/// error (the reference's `@reduce` refuses a zero-lane vector). Without this
/// the array port would silently return vacuous results.
///
/// ```compile_fail
/// use rustty_vt::page::{cell_field_mask, Cell, CellField, Mask};
/// type M = Mask<Cell, { cell_field_mask(&[CellField::ContentTag, CellField::StyleId]) }, 0>;
/// let cells: [Cell; 0] = [];
/// let _ = M::match_group(&cells, 0);
/// ```
pub struct Mask<T: Packed, const MASK: Backing, const GROUP_LEN: usize> {
    _p: core::marker::PhantomData<T>,
}

impl<T: Packed, const MASK: Backing, const GROUP_LEN: usize> Mask<T, MASK, GROUP_LEN> {
    /// The number of values processed at once by group operations.
    pub const GROUP_LEN: usize = GROUP_LEN;

    /// The field bits this mask covers.
    pub const MASK: Backing = MASK;

    /// PORT NOTE: the reference's group type is `@Vector(group_len, Backing)`
    /// and every group op reduces it with `@reduce`, which Zig rejects outright
    /// ("@reduce operation requires a vector with nonzero length") for a
    /// zero-lane vector. The zero-lane vector TYPE is legal there, so the error
    /// fires lazily — exactly when a group method is analyzed. `[Backing; 0]`
    /// has no such restriction, so the array port would compile the group ops
    /// and return vacuous results (`match_group`/`eql`/`eql_exact` all `true`,
    /// `eql_prefix` `0`). This associated const restores the reference's
    /// rejection: associated consts are evaluated at monomorphization only when
    /// referenced, and `load` (referenced below) is reached by all four group
    /// ops, so `GROUP_LEN == 0` is a compile error precisely when a group method
    /// is instantiated, while the scalar helpers (`bits`/`pattern`/`strip`/
    /// `match_scalar`/`eql_scalar`) stay usable.
    const GROUP_NONZERO: () = assert!(
        GROUP_LEN > 0,
        "Mask group operations require a nonzero group length"
    );

    /// Load a group of values from the slice starting at index `i`.
    /// Panics (asserts) unless at least `GROUP_LEN` values are available.
    #[inline]
    fn load(values: &[T], i: usize) -> [Backing; GROUP_LEN] {
        let () = Self::GROUP_NONZERO;
        let mut out = [0 as Backing; GROUP_LEN];
        let src = &values[i..i + GROUP_LEN];
        let mut k = 0;
        while k < GROUP_LEN {
            out[k] = src[k].to_bits();
            k += 1;
        }
        out
    }

    /// Returns the raw backing bits of a single value.
    #[inline]
    pub fn bits(v: T) -> Backing {
        v.to_bits()
    }

    /// Returns the masked bits of a single value: the bits of the
    /// masked fields with all other fields zeroed. Use this to
    /// build the expected value for the eql functions.
    #[inline]
    pub fn pattern(v: T) -> Backing {
        v.to_bits() & MASK
    }

    /// Returns the backing bits of a single value with the masked
    /// fields zeroed: the complement of `pattern`. Use this to
    /// compare values while ignoring the masked fields.
    #[inline]
    pub fn strip(v: T) -> Backing {
        v.to_bits() & !MASK
    }

    /// Returns true if every value in the group of group_len
    /// values starting at index i matches, where a value matches
    /// when none of the masked fields have any bits set: false
    /// for bools, zero for ints, the zero tag for enums, and so
    /// on. Asserts that at least group_len values are available.
    #[inline]
    pub fn match_group(values: &[T], i: usize) -> bool {
        let g = Self::load(values, i);
        let mut acc: Backing = 0;
        let mut k = 0;
        while k < GROUP_LEN {
            acc |= g[k];
            k += 1;
        }
        acc & MASK == 0
    }

    /// Scalar variant of `match_group` for a single value.
    #[inline]
    pub fn match_scalar(v: T) -> bool {
        v.to_bits() & MASK == 0
    }

    /// Returns true if the masked fields of every value in the
    /// group of group_len values starting at index i equal the
    /// expected pattern (see `pattern`).
    ///
    /// This is a masked compare: fields outside the mask may vary
    /// freely. Use this to detect runs of values that share the
    /// masked field contents while other fields differ, e.g. a run
    /// of cells with the same style ID but different codepoints.
    /// If the result you derive from a run depends on fields
    /// outside the mask, use `eqlExact` instead.
    #[inline]
    pub fn eql(values: &[T], i: usize, expected: Backing) -> bool {
        let g = Self::load(values, i);
        let mut ok = true;
        let mut k = 0;
        while k < GROUP_LEN {
            ok &= (g[k] & MASK) == expected;
            k += 1;
        }
        ok
    }

    /// Scalar variant of `eql` for a single value.
    #[inline]
    pub fn eql_scalar(v: T, expected: Backing) -> bool {
        Self::pattern(v) == expected
    }

    /// Returns true if any value in the group of group_len values
    /// starting at index i has masked fields equal to the expected
    /// pattern (see `pattern`). This is the "any" counterpart to
    /// `eql`: use it to detect the presence of a specific value
    /// within a group, e.g. a run scan that must stop when it
    /// encounters a sentinel codepoint anywhere in the group.
    #[inline]
    pub fn eql_any(values: &[T], i: usize, expected: Backing) -> bool {
        let g = Self::load(values, i);
        let mut ok = false;
        let mut k = 0;
        while k < GROUP_LEN {
            ok |= (g[k] & MASK) == expected;
            k += 1;
        }
        ok
    }

    /// Like `eql` but returns the number of leading values whose
    /// masked fields equal the expected pattern, i.e. group_len if
    /// the entire group matches. This is useful for early-exit run
    /// scans that need to pinpoint exactly where a run ends rather
    /// than only whether the whole group matches.
    #[inline]
    pub fn eql_prefix(values: &[T], i: usize, expected: Backing) -> usize {
        let g = Self::load(values, i);
        let mut ok = [false; GROUP_LEN];
        let mut all = true;
        let mut k = 0;
        while k < GROUP_LEN {
            ok[k] = (g[k] & MASK) == expected;
            all &= ok[k];
            k += 1;
        }

        // Test the whole group before extracting the prefix
        // count: turning a vector compare into a scalar bitmask
        // is expensive on some targets (e.g. NEON has no movemask
        // instruction) and run scans overwhelmingly see fully
        // matching groups, so we only pay for the extraction on
        // the final group of a run.
        if all {
            return GROUP_LEN;
        }
        let mut k = 0;
        while k < GROUP_LEN {
            if !ok[k] {
                return k;
            }
            k += 1;
        }
        GROUP_LEN
    }

    /// Returns true if every value in the group of group_len
    /// values starting at index i is bit-identical to the expected
    /// value. Note: this compares entire values; it is NOT
    /// affected by the field mask.
    ///
    /// This exists alongside `eql` for run detection where the
    /// derived result depends on fields outside the mask, so a
    /// masked compare would incorrectly extend the run. For
    /// example, the background color of a bg-color cell lives in
    /// the content field: two such cells only share a background
    /// if their content bits are identical, not merely their
    /// content tag. Values in such runs are typically produced by
    /// bulk fills (e.g. erase with a pending background) and are
    /// bit-identical in practice, so exact equality is both
    /// correct and cheap.
    #[inline]
    pub fn eql_exact(values: &[T], i: usize, expected: Backing) -> bool {
        let g = Self::load(values, i);
        let mut ok = true;
        let mut k = 0;
        while k < GROUP_LEN {
            ok &= g[k] == expected;
            k += 1;
        }
        ok
    }
}

// PORT NOTE: `@bitSizeOf(Row)` / `@bitSizeOf(Cell)`. Both are `packed
// struct(u64)`, so their bit size is exactly 64.
#[inline]
fn bit_size_of_row() -> usize {
    64
}
#[inline]
fn bit_size_of_cell() -> usize {
    64
}

// PORT NOTE: `StyleSet.base_align.toByteUnits()`, `GraphemeMap.base_align`,
// `hyperlink.Map.base_align`, `hyperlink.Set.base_align`. These are the offset-
// based structures' base alignments (max of header/key/value alignments).
fn style_set_base_align() -> usize {
    StyleSet::base_align()
}
fn grapheme_map_base_align() -> usize {
    GraphemeMap::base_align()
}
fn hyperlink_map_base_align() -> usize {
    hyperlink::Map::base_align()
}
fn hyperlink_set_base_align() -> usize {
    hyperlink::Set::base_align()
}

include!("page_impl.rs");

include!("page_tests.rs");
