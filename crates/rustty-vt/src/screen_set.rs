//! rustty-vt screen_set
//!
//! A ScreenSet holds multiple terminal screens. This is initially created
//! to handle simple primary vs alternate screens, but could be extended
//! in the future to handle N screens.
//!
//! One of the goals of this is to allow lazy initialization of screens
//! as needed. The primary screen is always initialized, but the alternate
//! screen may not be until first used.
//!
//! PORTS: ghostty src/terminal/ScreenSet.zig
//!
//! PORT STATUS: skeleton (vt-10) — types + method signatures; bodies `todo!()`
//! land in vt-11.
// PORT NOTE: this skeleton has `todo!()` method bodies (vt-11 lands the real
// bodies), so the parameters/imports they will consume are not yet read. The
// `unused_*` and `clippy::should_implement_trait` allows are skeleton-only and
// drop out once the bodies land.
#![allow(clippy::todo, dead_code, unused_variables, unused_imports)]
#![allow(clippy::should_implement_trait)]

// const std = @import("std");
// const assert = @import("../quirks.zig").inlineAssert;
// const lib = @import("lib.zig");
// const testing = std.testing;
// const Allocator = std.mem.Allocator;
// const Screen = @import("Screen.zig");
use crate::screen::Screen;

// PORT NOTE: the reference `Allocator.Error` maps to `pagelist::OutOfMemory`;
// the general-purpose allocator maps to `allocator_api2::alloc::Global`.
use crate::pagelist::OutOfMemory;
use allocator_api2::alloc::Global as Allocator;

/// The possible keys for screens in the screen set.
// PORT NOTE: `Key = lib.Enum(lib.target, &.{ "primary", "alternate" })`. This
// type is `#[repr(C)]` (primary=0, alternate=1) — it is used as an EnumMap key
// index and is the only C-ABI type in this cluster.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Primary = 0,
    Alternate = 1,
}

impl Key {
    /// Number of keys; used to size the per-key EnumMap storage.
    pub const COUNT: usize = 2;
}

/// A ScreenSet holds multiple terminal screens.
pub struct ScreenSet {
    /// The key value of the currently active screen. Useful for simple
    /// comparisons, e.g. "is this screen the primary screen".
    pub active_key: Key,

    /// The active screen pointer.
    pub active: *mut Screen,

    /// All screens that are initialized.
    // PORT NOTE: the reference `std.EnumMap(Key, *Screen)` ports to a per-key
    // array of optional stable screen pointers (`Option<*mut Screen>` per Key),
    // indexed by `Key as usize`. `active` aliases an entry in `all`; the
    // borrow-checker-safe key/index discipline is finalized in vt-11.
    pub all: [Option<*mut Screen>; Key::COUNT],

    /// Monotonic generation counter for each screen key. This changes whenever a
    /// screen is removed so external handles can distinguish a newly initialized
    /// screen from stale references into destroyed screen storage.
    // PORT NOTE: the reference `std.EnumMap(Key, usize)` ports to a per-key array
    // indexed by `Key as usize`.
    pub generations: [usize; Key::COUNT],
}

impl ScreenSet {
    pub fn init(alloc: Allocator, opts: crate::screen::Options) -> Result<ScreenSet, OutOfMemory> {
        todo!()
    }

    pub fn deinit(&mut self, alloc: Allocator) {
        todo!()
    }

    /// Get the screen for the given key, if it is initialized.
    pub fn get(&self, key: Key) -> Option<*mut Screen> {
        todo!()
    }

    /// Get the current generation for the given screen key.
    pub fn generation(&self, key: Key) -> usize {
        todo!()
    }

    /// Get the screen for the given key, initializing it if necessary.
    pub fn get_init(
        &mut self,
        alloc: Allocator,
        key: Key,
        opts: crate::screen::Options,
    ) -> Result<*mut Screen, OutOfMemory> {
        todo!()
    }

    /// Remove a key from the set. The primary screen cannot be removed (asserted).
    pub fn remove(&mut self, alloc: Allocator, key: Key) {
        todo!()
    }

    /// Switch the active screen to the given key. Requires that the
    /// screen is initialized.
    pub fn switch_to(&mut self, key: Key) {
        todo!()
    }
}
