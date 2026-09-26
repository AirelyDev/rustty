//! Highlights are any contiguous sequences of cells that should
//! be called out in some way, most commonly for text selection but
//! also search results or any other purpose.
//!
//! Within the terminal package, a highlight is a generic concept
//! that represents a range of cells.

// NOTE: The plan is for highlights to ultimately replace Selection
// completely. Selection is deeply tied to various parts of the Ghostty
// internals so this may take some time.
//
//! PORTS: ghostty src/terminal/highlight.zig
//!
//! PORT STATUS: complete (vt-13). The previously DEFERRED seam (`Tracked` +
//! `Untracked.track`) is now ported because the genuinely-unported sibling
//! `Screen` is available. `Flattened` is also ported here (the render state's
//! `updateHighlightsFlattened` consumes `highlight.Flattened`).

use crate::pagelist::iter::{Chunk as PageChunk, Direction};
use crate::pagelist::{List, Node, OutOfMemory, Pin};
use crate::screen::Screen;
use crate::size;

/// An untracked highlight is a highlight that stores its highlighted
/// area as a top-left and bottom-right screen pin. Since it is untracked,
/// the pins are only valid for the current terminal state and may not
/// be safe to use after any terminal modifications.
///
/// For rectangle highlights/selections, the downstream consumer of this
/// code is expected to interpret the pins in whatever shape they want.
/// For example, a rectangular selection would interpret the pins as
/// setting the x bounds for each row between start.y and end.y.
///
/// To simplify all operations, start MUST be before or equal to end.
//
// PORT NOTE: the reference `Untracked` is a plain value struct (Zig structs are
// trivially copyable). The Rust `Pin` is intentionally NOT `Copy`/`Clone` (it
// uses an explicit `pin_copy()` to keep raw-pointer copies auditable; see
// `pagelist/mod.rs`), so `Untracked` cannot derive `Copy`/`Clone` either. Its
// fields are `pub` and callers copy explicitly via `Pin::pin_copy`.
pub struct Untracked {
    pub start: Pin,
    pub end: Pin,
}

impl Untracked {
    // SB: `screen` is a raw `*mut Screen` (the reference's `*Screen`), NOT `&mut
    // Screen`. The searcher holds long-lived raw aliases into the same `Screen`
    // (PageListSearch.list, tracked pins); a `&mut *screen` reborrow here would
    // retag the whole `Screen` allocation and invalidate them under Stacked
    // Borrows (rust-mut-args-strongly-protected). `.pages` is reached via narrow
    // scoped reborrows off the raw pointer inside `Tracked::init`.
    #[allow(clippy::not_unsafe_ptr_arg_deref)] // reference takes `screen: *Screen` (raw)
    pub fn track(&self, screen: *mut Screen) -> Result<Tracked, OutOfMemory> {
        Tracked::init(screen, self.start, self.end)
    }

    pub fn eql(self, other: Untracked) -> bool {
        self.start.eql(other.start) && self.end.eql(other.end)
    }
}

/// A tracked highlight is a highlight that stores its highlighted
/// area as tracked pins within a screen.
///
/// A tracked highlight ensures that the pins remain valid even as
/// the terminal state changes. Because of this, tracked highlights
/// have more operations available to them.
///
/// There is more overhead to creating and maintaining tracked highlights.
/// If you're manipulating highlights that are untracked and you're sure
/// that the terminal state won't change, you can use the `initAssume`
/// function.
//
// PORT NOTE: `start`/`end` are tracked pins owned by `screen.pages` (returned by
// `track_pin` as `*mut Pin`), faithfully mirroring the reference `*Pin`.
pub struct Tracked {
    pub start: *mut Pin,
    pub end: *mut Pin,
}

impl Tracked {
    // SB: `screen` is `*mut Screen` (reference `*Screen`); `.pages` is reached via
    // narrow scoped `(*screen).pages` reborrows so no whole-`Screen` `&mut` retag
    // invalidates the searcher's sibling raw aliases (see `Untracked::track`).
    #[allow(clippy::not_unsafe_ptr_arg_deref)] // reference takes `screen: *Screen` (raw)
    pub fn init(screen: *mut Screen, start: Pin, end: Pin) -> Result<Tracked, OutOfMemory> {
        let start_tracked = unsafe { (*screen).pages.track_pin(start) }?;
        // errdefer screen.pages.untrackPin(start_tracked);
        let end_tracked = match unsafe { (*screen).pages.track_pin(end) } {
            Ok(p) => p,
            Err(e) => {
                unsafe { (*screen).pages.untrack_pin(start_tracked) };
                return Err(e);
            }
        };
        // errdefer screen.pages.untrackPin(end_tracked);
        Ok(Tracked {
            start: start_tracked,
            end: end_tracked,
        })
    }

    /// Initializes a tracked highlight by assuming that the provided
    /// pins are already tracked. This allows callers to perform tracked
    /// operations without the overhead of tracking the pins, if the
    /// caller can guarantee that the pins are already tracked or that
    /// the terminal state will not change.
    ///
    /// Do not call deinit on highlights created with this function.
    pub fn init_assume(start: *mut Pin, end: *mut Pin) -> Tracked {
        Tracked { start, end }
    }

    // SB: `screen` is `*mut Screen` (reference `*Screen`) — narrow `(*screen).pages`
    // reborrows only, never a whole-`Screen` `&mut` (see `Untracked::track`).
    #[allow(clippy::not_unsafe_ptr_arg_deref)] // reference takes `screen: *Screen` (raw)
    pub fn deinit(self, screen: *mut Screen) {
        unsafe { (*screen).pages.untrack_pin(self.start) };
        unsafe { (*screen).pages.untrack_pin(self.end) };
    }
}

/// A flattened highlight is a highlight that stores its highlighted
/// area as a list of page chunks. This representation allows for
/// traversing the entire highlighted area without needing to read any
/// terminal state or dereference any page nodes (which may have been
/// pruned).
//
// PORT NOTE: the reference `chunks` is a `std.MultiArrayList(Chunk)`; the Rust
// port stores the chunks as an array of structs (the SoA layout is a Zig
// storage optimization, not observable). `.slice().items(.node)` etc. become
// field projections over the slice.
//
// PORT NOTE (parity tail S7-6): the chunk storage `C` is either OWNED, an
// allocator-bound [`ChunkList`] (the default, what `clone(alloc)` returns and
// what the search result lists hold), or BORROWED, a `&[Chunk]`
// ([`FlattenedRef`]). The reference's `Flattened` is a plain value whose
// `chunks` header is copied freely: `SlidingWindow.next` returns one that
// aliases the window's `chunk_buf`, and `matchAt`/`selectedMatch` return
// shallow copies of stored results, with no allocation. The borrowed form is
// that shallow copy (its lifetime is the reference's "valid until the next
// call"); the owned form frees through the allocator it was built with.
pub struct Flattened<C = ChunkList> {
    /// The page chunks that make up this highlight. This handles the
    /// y bounds since chunks[0].start is the first highlighted row
    /// and chunks[len - 1].end is the last highlighted row (exclsive).
    pub chunks: C,

    /// The x bounds of the highlight. `bot_x` may be less than `top_x`
    /// for typical left-to-right highlights: can start the selection right
    /// of the end on a higher row.
    pub top_x: size::CellCountInt,
    pub bot_x: size::CellCountInt,
}

/// The owned chunk storage: a `std.MultiArrayList(Chunk)` through the
/// allocator it was built with.
pub type ChunkList = crate::alloc::AVec<Chunk>;

/// A shallow copy of a [`Flattened`] (the reference's by-value `Flattened`
/// that aliases another list's chunks).
pub type FlattenedRef<'a> = Flattened<&'a [Chunk]>;

/// A flattened chunk is almost identical to a PageList.Chunk but
/// we also flatten the serial number. This lets the flattened
/// highlight more robust for comparisons and validity checks with
/// the PageList.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Chunk {
    pub node: *mut Node,
    pub serial: u64,
    pub start: size::CellCountInt,
    pub end: size::CellCountInt,
}

/// `MultiArrayList(Chunk)`'s `init_capacity`: the largest field is 8 bytes
/// (`node`, `serial`), so `cache_line / 8`.
pub(crate) const CHUNK_INIT_CAPACITY: usize = crate::zig_list::init_capacity(8);

/// `ArrayList(Flattened)`'s `init_capacity`: Zig's `@sizeOf(Flattened)` is 32
/// (a 24-byte `MultiArrayList` header plus two `u16`s, padded to 8).
pub(crate) const FLATTENED_INIT_CAPACITY: usize = crate::zig_list::init_capacity(32);

impl Flattened {
    // pub const empty: Flattened = .{ .chunks = .empty, .top_x = 0, .bot_x = 0 };
    //
    // PORT NOTE: `.empty` holds no allocation; its list is bound to the Rust
    // heap handle until something is stored in it.
    pub fn empty() -> Flattened {
        Flattened {
            chunks: ChunkList::new_in(crate::alloc::VtAllocator::GLOBAL),
            top_x: 0,
            bot_x: 0,
        }
    }

    pub fn init(
        alloc: crate::alloc::VtAllocator,
        start: Pin,
        end: Pin,
    ) -> Result<Flattened, OutOfMemory> {
        // var result: std.MultiArrayList(PageChunk) = .empty;
        // errdefer result.deinit(alloc);
        // var it = start.pageIterator(.right_down, end);
        // while (it.next()) |chunk| try result.append(alloc, .{
        //     .node = chunk.node,
        //     .serial = chunk.node.serial,
        //     .start = chunk.start,
        //     .end = chunk.end,
        // });
        //
        // PORT NOTE: the errdefer is the list's drop (it frees through `alloc`).
        // `append` is `ensureUnusedCapacity(1)` + `setCapacity`, the
        // `MultiArrayList` growth.
        let mut result: ChunkList = ChunkList::new_in(alloc);
        let mut it = start.page_iterator(Direction::RightDown, Some(end));
        while let Some(chunk) = it.next() {
            let needed = result.len() + 1;
            crate::zig_list::multi_ensure_total_capacity(&mut result, needed, CHUNK_INIT_CAPACITY)?;
            result.push(Chunk {
                node: chunk.node,
                serial: unsafe { (*chunk.node).serial },
                start: chunk.start,
                end: chunk.end,
            });
        }
        Ok(Flattened {
            chunks: result,
            top_x: start.x,
            // PORT NOTE: the reference writes `.end_x = end.x` here, naming a field
            // `Flattened` does not have (`bot_x`). That is a latent compile error the
            // reference never hits because `Flattened.init` has no caller and Zig only
            // analyses referenced functions. `bot_x` is the field `empty` and `clone`
            // both use, so it is the intended one.
            bot_x: end.x,
        })
    }

    // pub fn deinit(self: *Flattened, alloc: Allocator) void {
    //     self.chunks.deinit(alloc);
    // }
    //
    // PORT NOTE: the list frees through the allocator it was built with; `alloc`
    // is checked against it (an empty list may still be on the `.empty` Rust
    // heap handle). The highlight is left empty.
    pub fn deinit(&mut self, alloc: crate::alloc::VtAllocator) {
        if self.chunks.capacity() != 0 {
            self.chunks.allocator().debug_assert_same(alloc);
        }
        self.chunks = ChunkList::new_in(alloc);
    }

    /// A shallow copy (the reference's by-value `Flattened`).
    pub fn as_ref(&self) -> FlattenedRef<'_> {
        Flattened {
            chunks: &self.chunks[..],
            top_x: self.top_x,
            bot_x: self.bot_x,
        }
    }
}

impl<C: core::ops::Deref<Target = [Chunk]>> Flattened<C> {
    // pub fn clone(self: *const Flattened, alloc: Allocator) Allocator.Error!Flattened {
    //     return .{
    //         .chunks = try self.chunks.clone(alloc),
    //         .top_x = self.top_x,
    //         .bot_x = self.bot_x,
    //     };
    // }
    //
    // PORT NOTE: the reference method is named `clone`; kept verbatim.
    // `#[allow(clippy::should_implement_trait)]` because its fallible,
    // allocator-taking signature can't be the std `Clone::clone`.
    #[allow(clippy::should_implement_trait)]
    pub fn clone(&self, alloc: crate::alloc::VtAllocator) -> Result<Flattened, OutOfMemory> {
        Ok(Flattened {
            chunks: crate::zig_list::multi_clone(&self.chunks, alloc, CHUNK_INIT_CAPACITY)?,
            top_x: self.top_x,
            bot_x: self.bot_x,
        })
    }

    pub fn start_pin(&self) -> Pin {
        Pin {
            node: self.chunks[0].node,
            x: self.top_x,
            y: self.chunks[0].start,
            garbage: false,
        }
    }

    pub fn end_pin(&self) -> Pin {
        let len = self.chunks.len();
        Pin {
            node: self.chunks[len - 1].node,
            x: self.bot_x,
            y: self.chunks[len - 1].end - 1,
            garbage: false,
        }
    }

    /// Convert to an Untracked highlight.
    pub fn untracked(&self) -> Untracked {
        // Note: we don't use startPin/endPin here because it is slightly
        // faster to reuse the slices.
        let nodes_len = self.chunks.len();
        Untracked {
            start: Pin {
                node: self.chunks[0].node,
                x: self.top_x,
                y: self.chunks[0].start,
                garbage: false,
            },
            end: Pin {
                node: self.chunks[nodes_len - 1].node,
                x: self.bot_x,
                y: self.chunks[nodes_len - 1].end - 1,
                garbage: false,
            },
        }
    }
}

// PORT NOTE: `PageChunk` is the reference `PageList.PageIterator.Chunk` alias
// used only by `Flattened.init`; re-exported for clarity that we faithfully
// translate that import.
#[allow(unused_imports)]
use List as _List;
#[allow(unused_imports)]
use PageChunk as _PageChunk;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alloc::testing::{with_handle, Config, CountingAllocator, FailingAllocator};
    use crate::alloc::Allocator;
    use crate::kitty;
    use crate::point;
    use crate::screen::{Options, Screen};

    fn test_screen() -> Screen {
        Screen::init(
            Allocator,
            Options {
                cols: 10,
                rows: 3,
                max_scrollback_bytes: Some(0),
                max_scrollback_lines: None,
                kitty_image_storage_limit: 0,
                kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
            },
        )
        .unwrap()
    }

    // PORT NOTE: net-new (highlight.zig declares no tests). It proves that
    // `Flattened.init`/`clone`'s declared `Allocator.Error` return is REACHABLE
    // (parity row S155-F6). Parity tail S7-6 replaced the port-only tripwire
    // (`FLATTENED_TW`) with a `FailingAllocator`, the reference's own tool.
    #[test]
    fn flattened_init_and_clone_propagate_out_of_memory() {
        let mut s = test_screen();
        let start = s
            .pages
            .pin(point::Point::Active(point::Coordinate { x: 0, y: 0 }))
            .unwrap();
        let end = s
            .pages
            .pin(point::Point::Active(point::Coordinate { x: 9, y: 2 }))
            .unwrap();

        let failing = FailingAllocator::over_heap(Config::default());
        let c = failing.allocator();
        with_handle(&c, |alloc| {
            // Unarmed: the same call succeeds and visits at least one chunk, so
            // the failure below is about the allocation and not about an empty
            // range.
            let ok = Flattened::init(alloc, start.pin_copy(), end.pin_copy()).unwrap();
            assert!(!ok.chunks.is_empty());

            failing.fail_index.set(failing.alloc_index.get());
            assert_eq!(
                Err(OutOfMemory),
                Flattened::init(alloc, start.pin_copy(), end.pin_copy()).map(|_| ()),
            );
            assert_eq!(Err(OutOfMemory), ok.clone(alloc).map(|_| ()));
            assert!(failing.has_induced_failure.get());
            drop(ok);
            assert_eq!(failing.allocated_bytes.get(), failing.freed_bytes.get());
        });

        s.deinit();
    }

    // Parity tail S7-6: `clone(alloc)` is `MultiArrayList.clone`: ONE
    // allocation of `growCapacity(len)` chunks (never a remap) through the
    // allocator it is given, freed through it; an empty highlight allocates
    // nothing; a shallow copy (`as_ref`) allocates nothing.
    #[test]
    fn flattened_clone_routes_through_the_given_allocator() {
        let mut s = test_screen();
        let start = s
            .pages
            .pin(point::Point::Active(point::Coordinate { x: 0, y: 0 }))
            .unwrap();
        let end = s
            .pages
            .pin(point::Point::Active(point::Coordinate { x: 9, y: 2 }))
            .unwrap();
        let src = Flattened::init(Allocator, start, end).unwrap();
        assert_eq!(1, src.chunks.len());

        let counting = CountingAllocator::over_heap();
        let c = counting.allocator();
        with_handle(&c, |alloc| {
            let shallow = src.as_ref();
            assert_eq!(0, counting.alloc_calls.get());
            let mut cloned = shallow.clone(alloc).unwrap();
            assert_eq!(
                (1, 0, 0),
                (
                    counting.alloc_calls.get(),
                    counting.remap_calls.get(),
                    counting.free_calls.get()
                )
            );
            // growCapacity(1) = 1 + 0 + 16 chunks, 8-aligned (log2 3).
            assert_eq!(
                Some((17 * core::mem::size_of::<Chunk>(), 3)),
                counting.last_alloc.get()
            );
            assert_eq!(src.chunks[..], cloned.chunks[..]);
            assert_eq!((src.top_x, src.bot_x), (cloned.top_x, cloned.bot_x));
            cloned.deinit(alloc);
            assert_eq!(0, counting.live_bytes.get());

            let empty = Flattened::empty().clone(alloc).unwrap();
            assert!(empty.chunks.is_empty());
            assert_eq!(1, counting.alloc_calls.get());
        });

        s.deinit();
    }
}
