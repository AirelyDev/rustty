//! A link is a clickable element that can be used to trigger some action.
//! A link is NOT just a URL that opens in a browser. A link is any generic
//! regular expression match over terminal text that can trigger various
//! action types.
//
// PORTS: ghostty src/input/Link.zig
//
// PORT STATUS: done
//
// PORT NOTE: the reference file IS the `Link` struct via
//   const Link = @This();
// and imports
//   const std = @import("std");
//   const Allocator = std.mem.Allocator;
//   const oni = @import("oniguruma");
//   const Mods = @import("key.zig").Mods;
// `std`/`Allocator` map to the Rust system allocator. `Mods` maps to the ported
// `rustty_vt::input::key_mods::Mods`. `oni` (Oniguruma) is NOT yet ported — the
// `oniguruma-sys` crate is a stub with no `oni.Regex` type. Following the
// established deferred-seam precedent (binding.rs `uucode`/`gobject`), `oniRegex`
// is ported as a deferred seam preserving the reference body and comments
// verbatim; it is satisfied when the Oniguruma binding lands. The `regex` byte
// slice (`[]const u8`) maps to an owned `Vec<u8>` for the stored field and a
// borrowed `&[u8]` ownership note (the reference: "Ownership of this memory is
// up to the caller. The link will never free this memory.").

use rustty_vt::input::key_mods::Mods;

/// PORT NOTE: the reference declares the link's fields at file scope (the file
/// IS the `Link` struct). Here they are the fields of an explicit `Link` struct.
#[derive(Clone, Debug, PartialEq)]
pub struct Link {
    /// The regular expression that will be used to match the link. Ownership
    /// of this memory is up to the caller. The link will never free this memory.
    pub regex: Vec<u8>,

    /// The action that will be triggered when the link is clicked.
    pub action: Action,

    /// The situations in which the link will be highlighted. A link is only
    /// clickable by the mouse when it is highlighted, so this also controls
    /// when the link is clickable.
    pub highlight: Highlight,
}

/// PORT NOTE: `Link.Action` — a `union(enum)` of void arms.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Open the full matched value using the default open program.
    /// For example, on macOS this is "open" and on Linux this is "xdg-open".
    Open,

    /// Open the OSC8 hyperlink under the mouse position. _-prefixed means
    /// this can't be user-specified, it's only used internally.
    OpenOsc8,
}

/// PORT NOTE: `Link.Highlight` — a `union(enum)`; `always`/`hover` are void
/// arms, `always_mods`/`hover_mods` carry a `Mods` payload.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Highlight {
    /// Always highlight the link.
    Always,

    /// Only highlight the link when the mouse is hovering over it.
    Hover,

    /// Highlight anytime the given mods are pressed, either when
    /// hovering or always. For always, all links will be highlighted
    /// when the mods are pressed regardless of if the mouse is hovering
    /// over them.
    ///
    /// Note that if "shift" is specified here, this will NEVER match in
    /// TUI programs that capture mouse events. "Shift" with mouse capture
    /// escapes the mouse capture but strips the "shift" so it can't be
    /// detected.
    AlwaysMods(Mods),
    HoverMods(Mods),
}

impl Link {
    /// Returns a new oni.Regex that can be used to match the link.
    //
    // PORT NOTE (DEFERRED SEAM, R4): the reference body is
    //   return try oni.Regex.init(
    //       self.regex,
    //       .{},
    //       oni.Encoding.utf8,
    //       oni.Syntax.default,
    //       null,
    //   );
    // The Oniguruma binding (`oni.Regex`/`oni.Encoding`/`oni.Syntax`) is not yet
    // ported (the `oniguruma-sys` crate is a C-binding stub). The full reference
    // call is preserved above; this seam is wired to the real binding when the
    // Oniguruma layer lands. No reference logic or comment is dropped.
    #[allow(clippy::result_unit_err)]
    pub fn oni_regex(&self) -> Result<OniRegex, ()> {
        // DEFERRED: requires the Oniguruma binding (see PORT NOTE above).
        unimplemented!("oni.Regex.init — deferred until the Oniguruma binding lands")
    }

    /// Deep clone the link.
    pub fn clone_in(&self) -> Link {
        // return .{
        //     .regex = try alloc.dupe(u8, self.regex),
        //     .action = self.action,
        //     .highlight = self.highlight,
        // };
        Link {
            regex: self.regex.clone(),
            action: self.action,
            highlight: self.highlight,
        }
    }

    /// Check if two links are equal.
    pub fn equal(&self, other: &Link) -> bool {
        // return std.meta.eql(self.action, other.action) and
        //     std.meta.eql(self.highlight, other.highlight) and
        //     std.mem.eql(u8, self.regex, other.regex);
        self.action == other.action
            && self.highlight == other.highlight
            && self.regex == other.regex
    }
}

// PORT NOTE (DEFERRED SEAM, R4): the reference's `oni.Regex` return type. A
// placeholder until the Oniguruma binding is ported (see `oni_regex`).
#[derive(Debug)]
pub enum OniRegex {}
