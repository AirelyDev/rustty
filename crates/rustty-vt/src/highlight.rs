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
// port uses a `Vec<Chunk>` (the SoA layout is a Zig storage optimization, not
// observable). `.slice().items(.node)` etc. become field projections over the
// `Vec`.
pub struct Flattened {
    /// The page chunks that make up this highlight. This handles the
    /// y bounds since chunks[0].start is the first highlighted row
    /// and chunks[len - 1].end is the last highlighted row (exclsive).
    pub chunks: Vec<Chunk>,

    /// The x bounds of the highlight. `bot_x` may be less than `top_x`
    /// for typical left-to-right highlights: can start the selection right
    /// of the end on a higher row.
    pub top_x: size::CellCountInt,
    pub bot_x: size::CellCountInt,
}

/// A flattened chunk is almost identical to a PageList.Chunk but
/// we also flatten the serial number. This lets the flattened
/// highlight more robust for comparisons and validity checks with
/// the PageList.
pub struct Chunk {
    pub node: *mut Node,
    pub serial: u64,
    pub start: size::CellCountInt,
    pub end: size::CellCountInt,
}

impl Flattened {
    // pub const empty: Flattened = .{ .chunks = .empty, .top_x = 0, .bot_x = 0 };
    pub fn empty() -> Flattened {
        Flattened {
            chunks: Vec::new(),
            top_x: 0,
            bot_x: 0,
        }
    }

    pub fn init(start: Pin, end: Pin) -> Result<Flattened, OutOfMemory> {
        // PORT NOTE: the reference accumulates into a `std.MultiArrayList(PageChunk)`
        // (with an `errdefer result.deinit(alloc)`); the Rust `Vec<Chunk>` owns its
        // storage and frees on drop, so the errdefer is implicit.
        let mut result: Vec<Chunk> = Vec::new();
        let mut it = start.page_iterator(Direction::RightDown, Some(end));
        while let Some(chunk) = it.next() {
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
            bot_x: end.x,
        })
    }

    // PORT NOTE: the reference `deinit` frees `self.chunks`; the Rust `Vec`
    // frees on drop, so an explicit method is a no-op kept for API parity.
    pub fn deinit(&mut self) {
        self.chunks = Vec::new();
    }

    // PORT NOTE: the reference method is named `clone` (returns
    // `Allocator.Error!Flattened`); kept verbatim. `#[allow(clippy::should_implement_trait)]`
    // because its fallible signature can't be the std `Clone::clone`.
    #[allow(clippy::should_implement_trait)]
    pub fn clone(&self) -> Result<Flattened, OutOfMemory> {
        let mut chunks: Vec<Chunk> = Vec::with_capacity(self.chunks.len());
        for c in &self.chunks {
            chunks.push(Chunk {
                node: c.node,
                serial: c.serial,
                start: c.start,
                end: c.end,
            });
        }
        Ok(Flattened {
            chunks,
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
