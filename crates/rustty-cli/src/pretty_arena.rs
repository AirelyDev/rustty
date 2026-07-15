//! rustty-cli::pretty_arena
//!
//! The faithful analogue of the `std.heap.ArenaAllocator` the reference
//! `list_colors`/`list_keybinds` `prettyPrint` threads into its render body
//! (`arena.allocator()`): a bump-lifetime byte arena whose duped slices stay
//! valid for the arena's whole lifetime.
//!
//! WHY IT EXISTS (WIRE1): the vaxis `Window` stores each printed `Segment`'s
//! `text: &'a [u8]` BY REFERENCE into the screen's `Cell<'a>` buffer (§5). So the
//! transient bytes the reference builds with `std.fmt.allocPrint(alloc, ...)`
//! (the `#rrggbb` hex, the `{t}`/`{u}` key strings, the `{f}` action strings)
//! must outlive the screen — exactly what the reference arena guarantees. Each
//! `allocPrint(alloc, ...)` maps 1:1 to an [`Arena::dupe`], and the arena is
//! dropped after the `Vaxis` (declared before it), mirroring the reference
//! `defer arena.deinit()` firing after the vx/tty defers.

use crate::rustty::Error;
use rustty_vaxis::cell::{Segment, Style};
use rustty_vaxis::window::PrintOptions;
use rustty_vaxis::Winsize;
use std::cell::RefCell;

// const winsize: vaxis.Winsize = switch (builtin.os.tag) { ... };
//   PORT NOTE: the reference's `builtin.os.tag` switch — `.windows` uses a fixed
//   default (we don't wrap, so it doesn't matter), `else` reads the real winsize.
//   Shared verbatim by both `list_colors` and `list_keybinds` `prettyPrint`.
pub fn winsize_for(tty: &rustty_vaxis::Tty) -> Result<Winsize, Error> {
    // We use some default, it doesn't really matter for what
    // we're doing because we don't do any wrapping.
    // .windows => .{ .rows = 24, .cols = 120, .x_pixel = 1024, .y_pixel = 768 },
    #[cfg(windows)]
    {
        let _ = tty;
        Ok(Winsize {
            rows: 24,
            cols: 120,
            x_pixel: 1024,
            y_pixel: 768,
        })
    }
    // else => try vaxis.Tty.getWinsize(tty.fd),
    #[cfg(not(windows))]
    {
        Ok(rustty_vaxis::Tty::get_winsize(tty.fd())?)
    }
}

/// `.{ .text = t }` — a bare segment (default style + no hyperlink).
pub fn seg(text: &[u8]) -> Segment<'_> {
    Segment {
        text,
        style: Style::default(),
        link: Default::default(),
    }
}

/// `.{ .text = t, .style = s }` — a styled segment.
pub fn styled(text: &[u8], style: Style) -> Segment<'_> {
    Segment {
        text,
        style,
        link: Default::default(),
    }
}

/// `.{ .col_offset = c }` — the `printSegment`/`print` position option.
pub fn po(col: u16) -> PrintOptions {
    PrintOptions {
        col_offset: col,
        ..Default::default()
    }
}

/// A bump-lifetime byte arena. `dupe` copies bytes into an owned buffer and
/// returns a reference valid for the arena's lifetime.
#[derive(Default)]
pub struct Arena {
    // Each duped slice is a separate `Vec<u8>` heap buffer; the outer `Vec` only
    // stores the inner `Vec` handles. Growing the outer `Vec` MOVES the inner
    // `Vec` handles but never their heap buffers, and — unlike `Box` — moving a
    // `Vec` does not retag its pointee, so outstanding `dupe` references stay
    // valid under Stacked Borrows.
    chunks: RefCell<Vec<Vec<u8>>>,
}

impl Arena {
    // var arena = std.heap.ArenaAllocator.init(alloc);
    pub fn init() -> Arena {
        Arena {
            chunks: RefCell::new(Vec::new()),
        }
    }

    /// `std.fmt.allocPrint(arena.allocator(), ...)` / `arena.allocator().dupe(...)`:
    /// copy `bytes` into the arena and return a reference living as long as the arena.
    pub fn dupe<'a>(&'a self, bytes: &[u8]) -> &'a [u8] {
        let owned: Vec<u8> = bytes.to_vec();
        // Raw pointer to the heap buffer (stable for the arena's lifetime).
        let ptr: *const [u8] = owned.as_slice();
        self.chunks.borrow_mut().push(owned);
        // SAFETY: `owned`'s heap buffer that `ptr` points at is not moved when the
        // `Vec` handle is pushed into `chunks` (only the 3-word handle moves, and
        // moving a `Vec` — not a `Box` — performs no `noalias` retag of the
        // pointee). Later `dupe`s may reallocate the outer `Vec`, but that again
        // only moves inner handles, never the buffers. The arena never frees a
        // buffer before its own `Drop`, which happens after the borrow `'a` ends.
        // This is the standard typed-arena chunk pattern; Stacked-Borrows / Miri clean.
        unsafe { &*ptr }
    }
}
