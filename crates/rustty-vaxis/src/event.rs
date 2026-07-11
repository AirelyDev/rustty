//! Port of `libvaxis` `event.zig` (30 LOC) — the `Event` union (the events Vaxis
//! emits internally) plus the re-exports of `Key`/`Mouse`/`Color`/`Winsize`.
//!
//! PORT NOTE (variant casing): the Zig union fields are snake_case (`key_press`,
//! `cap_kitty_keyboard`); Rust enum variants are CamelCase (`KeyPress`,
//! `CapKittyKeyboard`). Faithful 1:1 rename.
//!
//! PORT NOTE (paste ownership): Zig `paste: []const u8` is a base64-decoded
//! allocation the caller must free (`osc 52 paste, caller must free`). Rust owns it
//! as `Paste(Vec<u8>)` — it drops itself, so there is no "caller must free" caveat
//! and `Event` is NOT `Copy`. Grapheme bytes are `&[u8]`, never a lossy `String`.

use crate::cell;

pub use crate::cell::Color; // Zig `pub const Color = @import("Cell.zig").Color;`
pub use crate::key::Key;
pub use crate::mouse::Mouse;
pub use crate::Winsize;

/// The events that Vaxis emits internally
#[derive(Clone, Debug, PartialEq)]
pub enum Event<'a> {
    KeyPress(Key<'a>),
    KeyRelease(Key<'a>),
    Mouse(Mouse),
    MouseLeave,
    FocusIn,
    FocusOut,
    PasteStart,                // bracketed paste start
    PasteEnd,                  // bracketed paste end
    Paste(Vec<u8>),            // osc 52 paste, caller must free
    ColorReport(cell::Report), // osc 4, 10, 11, 12 response
    ColorScheme(cell::Scheme),
    Winsize(Winsize),

    // these are delivered as discovered terminal capabilities
    CapKittyKeyboard,
    CapKittyGraphics,
    CapRgb,
    CapSgrPixels,
    CapUnicode,
    CapDa1,
    CapColorSchemeUpdates,
    CapMultiCursor,
}
