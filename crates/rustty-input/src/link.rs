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
// `rustty_vt::input::key_mods::Mods`. `oni` (Oniguruma) maps to the ported `oni`
// safe-wrapper crate (`rustty-oniguruma`, lib name `oni`); referenced directly as
// `oni::*` (no `use`, mirroring the reference's `const oni = @import("oniguruma")`
// module binding). The reference imports `oniguruma` UNCONDITIONALLY — Link is
// core input, NOT behind `build_options.oniguruma`; only the standalone
// libghostty-vt build disables oniguruma (which is why the *vt* file
// `terminal/StringMap.zig` gates its regex surface on `build_options.oniguruma`),
// while `src/input/` is always built with oniguruma present. So `rustty-oniguruma`
// is a NON-optional dependency of `rustty-input` and `oni_regex` is always
// compiled (no cargo feature gate). The `regex` byte slice (`[]const u8`) maps to
// an owned `Vec<u8>` for the stored field and a borrowed `&[u8]` ownership note
// (the reference: "Ownership of this memory is up to the caller. The link will
// never free this memory.").

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
    // PORT NOTE (WS14 T6, `#[cfg(feature = "oniguruma")]`): the reference
    // `Link.oniRegex` is UNCONDITIONAL — `src/input/` is always built with
    // oniguruma present. The default-ON cargo feature exists solely because the
    // `oniguruma-sys` C build script cannot target `wasm32-unknown-unknown` (no
    // libc sysroot), and this method is unreachable there regardless: link
    // highlighting is a renderer concern and the wasm module has no renderer. Every
    // native build enables the feature by default, so nothing changes off-wasm.
    #[cfg(feature = "oniguruma")]
    // PORT NOTE: `!oni.Regex` maps to `Result<oni::Regex, oni::Error>`; the
    // `.{}` (2nd arg) is the default `RegexOptions`, `null` (5th arg) the absent
    // `?*ErrorInfo` -> `None`. `oni.Encoding.utf8`/`oni.Syntax.default` are the
    // `oni::Encoding::utf8()`/`oni::Syntax::default()` accessors.
    pub fn oni_regex(&self) -> Result<oni::Regex, oni::Error> {
        // return try oni.Regex.init(
        //     self.regex,
        //     .{},
        //     oni.Encoding.utf8,
        //     oni.Syntax.default,
        //     null,
        // );
        //
        // PORT NOTE (P8-W9 FX4, NEW-FINDINGS N540; the N102 precedent in
        // rustty-vt's `tmux::control`): `oni.Regex.init` -> `onig_new`
        // initializes Oniguruma's process-global tables LAZILY when nothing has
        // (`onig_reg_init` reads `onig_inited`, then runs `onig_initialize`),
        // with no lock. The reference never reaches that path: the app runs
        // `oni.init(&.{oni.Encoding.utf8})` once at startup (`global.zig`),
        // before any surface derives its config, and `zig test` is serial. Rust
        // runs `DerivedConfig::init` (the surface's and the renderer's) on many
        // test threads, beside tests that call `ensure_init`, and G-TSAN
        // reported the lazy init racing the explicit one. So the one
        // thread-safe process-global init runs first. It is the same UTF-8-only
        // init the app performs (`rustty-apprt` `GlobalState::init` routes its
        // `oni.init` through it), so after the app's call this is a no-op. Its
        // error is ignored: `Regex::init` below still reports an init failure
        // through the reference's own error path.
        let _ = oni::testing::ensure_init();
        oni::Regex::init(
            &self.regex,
            oni::RegexOptions::default(),
            oni::Encoding::utf8(),
            oni::Syntax::default(),
            None,
        )
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
