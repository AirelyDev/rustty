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
//! PORT STATUS: complete (vt-11g) — types + all method bodies + the 2 tests.
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
        // We need to initialize our initial primary screen
        // const screen = try alloc.create(Screen);
        // errdefer alloc.destroy(screen);
        // screen.* = try .init(alloc, opts);
        // PORT NOTE: the reference `alloc.create(Screen)` allocates uninitialized
        // storage and the following `screen.* = try .init(...)` populates it (with
        // an `errdefer alloc.destroy(screen)` covering the init failure). `Screen`
        // has no self-referential pointers that require a fixed address before
        // init, so the faithful Rust ordering is to construct the value first and
        // only heap-allocate on success — that subsumes the `errdefer` (no leak on
        // the init-error path).
        let screen: *mut Screen = Box::into_raw(Box::new(Screen::init(alloc, opts)?));
        let mut all: [Option<*mut Screen>; Key::COUNT] = [None; Key::COUNT];
        all[Key::Primary as usize] = Some(screen);
        Ok(ScreenSet {
            active_key: Key::Primary,
            active: screen,
            all,
            generations: [0; Key::COUNT],
        })
    }

    pub fn deinit(&mut self, alloc: Allocator) {
        let _ = alloc;
        // Destroy all initialized screens
        // var it = self.all.iterator();
        // while (it.next()) |entry| {
        //     entry.value.*.deinit();
        //     alloc.destroy(entry.value.*);
        // }
        for slot in self.all.iter_mut() {
            if let Some(screen) = *slot {
                // SAFETY: `screen` was produced by `Box::into_raw` in
                // `init`/`get_init` and has not yet been freed; reclaim ownership
                // so the `Box` drop both `deinit`s and frees it. `deinit` takes
                // `&mut Screen`.
                let mut boxed = unsafe { Box::from_raw(screen) };
                boxed.deinit();
                drop(boxed);
            }
        }
    }

    /// Get the screen for the given key, if it is initialized.
    pub fn get(&self, key: Key) -> Option<*mut Screen> {
        self.all[key as usize]
    }

    /// Get the current generation for the given screen key.
    pub fn generation(&self, key: Key) -> usize {
        self.generations[key as usize]
    }

    /// Get the screen for the given key, initializing it if necessary.
    pub fn get_init(
        &mut self,
        alloc: Allocator,
        key: Key,
        opts: crate::screen::Options,
    ) -> Result<*mut Screen, OutOfMemory> {
        if let Some(screen) = self.get(key) {
            return Ok(screen);
        }
        // const screen = try alloc.create(Screen);
        // errdefer alloc.destroy(screen);
        // screen.* = try .init(alloc, opts);
        // PORT NOTE: same create+init ordering as `init` (construct first, only
        // heap-allocate on success — subsumes the reference's `errdefer`).
        let screen: *mut Screen = Box::into_raw(Box::new(Screen::init(alloc, opts)?));
        self.all[key as usize] = Some(screen);
        Ok(screen)
    }

    /// Remove a key from the set. The primary screen cannot be removed (asserted).
    pub fn remove(&mut self, alloc: Allocator, key: Key) {
        let _ = alloc;
        debug_assert!(key != Key::Primary);
        // if (self.all.fetchRemove(key)) |screen| {
        //     self.generations.put(key, self.generation(key) +% 1);
        //     screen.deinit();
        //     alloc.destroy(screen);
        // }
        if let Some(screen) = self.all[key as usize].take() {
            self.generations[key as usize] = self.generation(key).wrapping_add(1);
            // SAFETY: `screen` was produced by `Box::into_raw` and removed from
            // `all` via `take`, so this is the unique owner; reclaim it to
            // `deinit` and free.
            let mut boxed = unsafe { Box::from_raw(screen) };
            boxed.deinit();
            drop(boxed);
        }
    }

    /// Switch the active screen to the given key. Requires that the
    /// screen is initialized.
    pub fn switch_to(&mut self, key: Key) {
        self.active_key = key;
        self.active = self.all[key as usize].unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::screen::Options;
    use allocator_api2::alloc::Global;

    // test ScreenSet
    #[test]
    fn screen_set() {
        let alloc = Global;
        let mut set: ScreenSet = ScreenSet::init(alloc, Options::default()).unwrap();
        assert_eq!(set.active_key, Key::Primary);
        assert_eq!(set.generation(Key::Primary), 0_usize);
        assert_eq!(set.generation(Key::Alternate), 0_usize);

        // Initialize a secondary screen
        let _ = set
            .get_init(alloc, Key::Alternate, Options::default())
            .unwrap();
        assert_eq!(set.generation(Key::Alternate), 0_usize);

        set.switch_to(Key::Alternate);
        assert_eq!(set.active_key, Key::Alternate);

        set.deinit(alloc);
    }

    // test "ScreenSet generations"
    #[test]
    fn screen_set_generations() {
        let alloc = Global;
        let mut set: ScreenSet = ScreenSet::init(alloc, Options::default()).unwrap();

        assert_eq!(set.generation(Key::Primary), 0_usize);
        assert_eq!(set.generation(Key::Alternate), 0_usize);

        // A no-op removal doesn't change the generation.
        set.remove(alloc, Key::Alternate);
        assert_eq!(set.generation(Key::Alternate), 0_usize);

        // Initializing a screen doesn't change the generation.
        let _ = set
            .get_init(alloc, Key::Alternate, Options::default())
            .unwrap();
        assert_eq!(set.generation(Key::Alternate), 0_usize);

        let alternate_generation = set.generation(Key::Alternate);
        set.remove(alloc, Key::Alternate);
        assert_eq!(
            set.generation(Key::Alternate),
            alternate_generation.wrapping_add(1)
        );

        // Reinitializing keeps the generation from the last removal, so stale
        // handles can distinguish the new screen from the destroyed screen.
        let _ = set
            .get_init(alloc, Key::Alternate, Options::default())
            .unwrap();
        assert_eq!(
            set.generation(Key::Alternate),
            alternate_generation.wrapping_add(1)
        );
        assert_eq!(set.generation(Key::Primary), 0_usize);

        set.deinit(alloc);
    }
}
