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
//! PORT STATUS: partial. The `Untracked` highlight struct (data + `eql`) is
//! ported here because the access cluster's `PageList.highlightSemanticContent`
//! returns `?highlight.Untracked`. The `Tracked` highlight and
//! `Untracked.track` are a DOCUMENTED DEFERRED SEAM: they require the genuinely
//! unported sibling `Screen` (`@import("Screen.zig")`) and have no ported
//! caller yet, so they land when the `Screen` port lands — NOT stubbed.

use crate::pagelist::Pin;

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
    // PORT NOTE (DEFERRED SEAM): the reference `Untracked.track` returns
    // `Allocator.Error!Tracked` and forwards to `Tracked.init(screen, ...)`.
    // `Tracked` and its `init` require the genuinely unported sibling `Screen`
    // (`@import("Screen.zig")`). No ported caller invokes `track` yet, so it is
    // deferred to the `Screen` port and lands there fully ported — not stubbed.
    //
    // pub fn track(
    //     self: *const Untracked,
    //     screen: *Screen,
    // ) Allocator.Error!Tracked {
    //     return try .init(
    //         screen,
    //         self.start,
    //         self.end,
    //     );
    // }

    pub fn eql(self: Untracked, other: Untracked) -> bool {
        self.start.eql(other.start) && self.end.eql(other.end)
    }
}
