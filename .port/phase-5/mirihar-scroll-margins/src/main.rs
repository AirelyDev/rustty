//! Isolated, non-FFI Miri harness for the P5-W2.3 ScrollMargins guard
//! (upstream `90bce0d2d` "scroll and clip image placements within margins";
//! phase-5 plan SB/UB hazard #1).
//!
//! It faithfully models the exact aliasing structure of
//! `crates/rustty-vt/src/kitty/graphics_storage.rs`
//! (`ImageStorage::scroll_margins_begin` / `ScrollMargins::drop`) wired
//! through `crates/rustty-vt/src/terminal.rs`
//! (`kitty_scroll_margins_begin` → row operation → guard drop):
//!
//!   * The `Screen` is a SEPARATE heap allocation reached from `Terminal`
//!     only through the raw `screens.active: *mut Screen` (rustty's real
//!     screen-set shape). `ImageStorage` lives INSIDE that screen
//!     (`screen.kitty_images`), so `scroll_margins_begin(&mut self, t: *mut
//!     Terminal, ..)` is the classic storage-inside-screen self-alias: the
//!     `&mut` covers only the `kitty_images` field range while `(*t)` /
//!     `(*s)` raw reads touch the disjoint Terminal allocation and the
//!     screen's OTHER fields (`pages`).
//!   * Tracked pins are pool-owned heap allocations addressed by raw
//!     `*mut Pin` (rustty `PageList::track_pin`). The guard records those
//!     raw pointers PLUS target coordinates at begin time, stays alive
//!     across the caller's `&mut Terminal` row operation — which itself
//!     mutates every tracked pin through the pagelist's OWN raw-pointer
//!     path — and only dereferences its pointers in `end()` (Drop): a
//!     shared `pages` lookup followed by a raw write per pin.
//!   * begin itself interleaves: shared reads of `(*t)` geometry, shared
//!     reads through `*mut Pin`, `&mut` access to the placements map
//!     (clip mutation), and a placement REMOVAL that untracks a pin via
//!     `(*s).pages` — all while `&mut kitty_images` is live. That is
//!     exactly the field-disjoint raw-projection discipline the real fn
//!     uses (`rust-mut-args-strongly-protected`).
//!   * The upstream implicit contract written down on the real guard: NO
//!     storage mutation between begin and end, so no RECORDED pin is
//!     untracked (freed) while the guard holds its address. The foil shows
//!     Miri catching exactly that violation.
//!
//! Candidates (argv[1]; Miri only checks executed code):
//!   * `ported`              — the shipped discipline: begin (with a
//!                             mid-iteration clip mutation AND a placement
//!                             deletion that untracks a NON-recorded pin),
//!                             a `&mut Terminal` row op that rewrites every
//!                             tracked pin via the pagelist's own pointers,
//!                             then the guard drop re-deriving and writing
//!                             the recorded pins. MUST be clean under
//!                             Stacked Borrows and Tree Borrows.
//!   * `ub_end_after_untrack` — DOCUMENTED-UB foil: a recorded pin is
//!                             untracked (its pool allocation freed)
//!                             between begin and end — the contract
//!                             violation the no-mutation window prevents —
//!                             and end() writes through the dangling
//!                             pointer. Miri flags the use-after-free.
//!
//! Run (from this dir):
//! ```sh
//! cargo +nightly miri run -q -- ported                                 # Stacked Borrows
//! MIRIFLAGS=-Zmiri-tree-borrows cargo +nightly miri run -q -- ported   # Tree Borrows
//! cargo +nightly miri run -q -- ub_end_after_untrack                   # expected: UB report
//! ```
#![allow(dead_code)]

use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Model types mirroring the real field/allocation structure.

/// `PageList::Pin` stand-in: pool-owned, addressed by raw pointer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Pin {
    x: u16,
    y: u32,
    garbage: bool,
}

/// The pagelist: owns the tracked-pin pool (`track_pin`/`untrack_pin`) and
/// the `pin(point)` lookup the guard's end() uses.
struct Pages {
    tracked: Vec<*mut Pin>,
}

impl Pages {
    fn track_pin(&mut self, p: Pin) -> *mut Pin {
        let ptr = Box::into_raw(Box::new(p));
        self.tracked.push(ptr);
        ptr
    }

    fn untrack_pin(&mut self, p: *mut Pin) {
        if let Some(idx) = self.tracked.iter().position(|&q| q == p) {
            self.tracked.swap_remove(idx);
            // SAFETY: `p` came from `Box::into_raw` in `track_pin` and is
            // removed from the tracked set exactly once.
            drop(unsafe { Box::from_raw(p) });
        }
    }

    /// `pages.pin(.{ .active = .{x, y} })` analog: a by-value lookup.
    fn pin(&self, x: u16, y: u32) -> Option<Pin> {
        Some(Pin {
            x,
            y,
            garbage: false,
        })
    }

    /// The row operation's pin adjustment: the pagelist mutates every
    /// tracked pin through its OWN raw-pointer path (rustty's row ops do
    /// exactly this via the tracked-pin set).
    fn shift_all_pins(&mut self, delta: i64) {
        for &p in &self.tracked {
            // SAFETY: every pointer in `tracked` is a live pool allocation.
            unsafe {
                (*p).y = ((*p).y as i64 + delta).max(0) as u32;
            }
        }
    }
}

/// `Screen` stand-in: the pagelist plus the kitty image storage INSIDE it.
struct Screen {
    pages: Pages,
    kitty_images: ImageStorage,
}

/// `Placement` stand-in: holds the raw tracked-pin pointer plus a clippable
/// field (`source_y` mirrors the clip_top mutation).
struct Placement {
    pin: *mut Pin,
    source_y: u32,
}

impl Placement {
    /// `Placement::deinit(s)` — release the tracked pin via the screen.
    fn deinit(&self, s: *mut Screen) {
        // SAFETY: `s` is the owning screen; narrow `&mut` projection of its
        // `pages` field only (the real fn's SAFETY contract).
        unsafe { (*s).pages.untrack_pin(self.pin) }
    }
}

/// `ImageStorage` stand-in.
struct ImageStorage {
    placements: HashMap<u32, Placement>,
    dirty: bool,
}

/// `ScrollMargins` guard: raw screen + raw pins, deref'd only in Drop.
struct ScrollMargins {
    screen: *mut Screen,
    restores: Vec<Restore>,
}

struct Restore {
    pin: *mut Pin,
    x: u16,
    y: u32,
}

impl ScrollMargins {
    fn end(self) {}
}

impl Drop for ScrollMargins {
    fn drop(&mut self) {
        for restore in &self.restores {
            // Shared `pages` projection for the lookup, then the raw pin
            // write — the real `ScrollMargins::drop` sequence.
            // SAFETY: mirrors the real guard's contract: `screen` outlives
            // the guard, and every recorded pin is still pool-live (no
            // storage mutation between begin and end).
            let new_pin = match unsafe { (*self.screen).pages.pin(restore.x, restore.y) } {
                Some(p) => p,
                None => continue,
            };
            unsafe { *restore.pin = new_pin };
        }
    }
}

impl ImageStorage {
    /// `scroll_margins_begin` with the real function's exact access
    /// interleaving: `&mut self` (a field of the screen `(*t).screens.active`
    /// points to) live across raw reads of the terminal, shared reads through
    /// tracked-pin pointers, map mutation, and a pin-untracking removal.
    fn scroll_margins_begin(&mut self, t: *mut Terminal, delta: i64) -> ScrollMargins {
        // SAFETY: `t` is the terminal owning this storage's screen; field
        // reads only (the real fn's SAFETY contract).
        let s: *mut Screen = unsafe { (*t).screens.active };
        // assert(self == &s.kitty_images);
        assert!(std::ptr::eq(self as *const ImageStorage, unsafe {
            &raw const (*s).kitty_images
        }));
        let mut result = ScrollMargins {
            screen: s,
            restores: Vec::new(),
        };

        let top: i64 = unsafe { (*t).scrolling_region_top } as i64;
        let bottom: i64 = unsafe { (*t).scrolling_region_bottom } as i64;

        let mut mutated = false;
        let keys: Vec<u32> = self.placements.keys().copied().collect();
        'placements: for key in keys {
            let pin: *mut Pin = self.placements.get(&key).unwrap().pin;

            // Shared read through the raw pin while `&mut self` is live.
            // SAFETY: tracked pin, no `&mut` to it live.
            if unsafe { (*pin).garbage } {
                continue;
            }
            let y: i64 = unsafe { (*pin).y } as i64;

            let mut final_y: i64 = y;
            'inside: {
                if y < top || y > bottom {
                    break 'inside;
                }
                let new_y = y + delta;
                if new_y < top - 1 {
                    // Fully scrolled out: delete the placement (untracks its
                    // pin through the screen — the raw-projection interleave).
                    self.placements.get(&key).unwrap().deinit(s);
                    self.placements.remove(&key);
                    mutated = true;
                    continue 'placements;
                }
                if new_y < top {
                    // Clip: mutate the placement value in the map while the
                    // raw `t`/`s` pointers stay usable afterwards.
                    self.placements.get_mut(&key).unwrap().source_y += 10;
                    mutated = true;
                    final_y = top;
                    break 'inside;
                }
                final_y = new_y;
            }

            result.restores.push(Restore {
                pin,
                // SAFETY: shared read of the tracked pin.
                x: unsafe { (*pin).x },
                y: u32::try_from(final_y).unwrap(),
            });
        }

        if mutated {
            self.dirty = true;
        }
        result
    }
}

/// `Terminal` stand-in: reaches its screen only through the raw pointer.
struct Terminal {
    screens: Screens,
    scrolling_region_top: u32,
    scrolling_region_bottom: u32,
}

struct Screens {
    active: *mut Screen,
}

impl Terminal {
    fn new() -> Terminal {
        let screen = Box::into_raw(Box::new(Screen {
            pages: Pages {
                tracked: Vec::new(),
            },
            kitty_images: ImageStorage {
                placements: HashMap::new(),
                dirty: false,
            },
        }));
        Terminal {
            screens: Screens { active: screen },
            scrolling_region_top: 1,
            scrolling_region_bottom: 4,
        }
    }

    /// `kitty_scroll_margins_begin` — the real call shape: raw `t` from
    /// `&mut self`, narrow `&mut` projection of the screen's storage field.
    fn kitty_scroll_margins_begin(&mut self, delta: i64) -> ScrollMargins {
        let t: *mut Terminal = self;
        let screen: *mut Screen = self.screens.active;
        // SAFETY: field-disjoint projection; see the real fn's SAFETY note.
        unsafe { &mut (*screen).kitty_images }.scroll_margins_begin(t, delta)
    }

    /// The row operation between begin and end: a full `&mut Terminal` path
    /// that rewrites every tracked pin via the pagelist's own pointers.
    fn row_op(&mut self, delta: i64) {
        let screen = self.screens.active;
        // SAFETY: narrow `&mut` projection of the screen's `pages` field.
        unsafe { (*screen).pages.shift_all_pins(delta) };
    }

    fn deinit(self) {
        // SAFETY: `active` came from Box::into_raw in `new`; free the pins
        // then the screen exactly once.
        unsafe {
            let screen = Box::from_raw(self.screens.active);
            for p in screen.pages.tracked.iter() {
                drop(Box::from_raw(*p));
            }
        }
    }
}

// ---------------------------------------------------------------------------

fn add_placement(t: &mut Terminal, key: u32, x: u16, y: u32) {
    let screen = t.screens.active;
    // SAFETY: narrow projections of the screen's two fields, sequentially.
    let pin = unsafe { (*screen).pages.track_pin(Pin { x, y, garbage: false }) };
    unsafe {
        (*screen)
            .kitty_images
            .placements
            .insert(key, Placement { pin, source_y: 0 });
    }
}

/// The shipped discipline. Must be Miri-clean (SB and TB).
fn ported() {
    let mut t = Terminal::new();
    // Region rows 1..=4. Three placements: one scrolls in-region (key 1),
    // one clips at the top margin (key 2), one is deleted mid-begin (key 3).
    add_placement(&mut t, 1, 0, 3);
    add_placement(&mut t, 2, 2, 1);
    add_placement(&mut t, 3, 5, 1);

    // begin(delta = -1): key 1 → y 2; key 2 → clip, restored to top; key 3
    // (given delta -3 semantics via a second pass) stays for this pass.
    let guard = t.kitty_scroll_margins_begin(-1);
    // The row operation runs with the guard live, dragging every tracked
    // pin through the pagelist's own pointer path.
    t.row_op(-1);
    guard.end();

    // A second scroll big enough to delete key 3 (new_y < top-1 for y=0
    // after the first pass moved it): exercises the untrack-during-begin
    // interleave for a NON-recorded pin.
    let guard = t.kitty_scroll_margins_begin(-3);
    t.row_op(-3);
    guard.end();

    // Verify final coordinates through fresh shared reads.
    let screen = t.screens.active;
    let storage = unsafe { &(*screen).kitty_images };
    assert!(storage.dirty);
    // key 2 clipped once, then deleted by the -3 pass (y=1 → new_y=-2).
    // key 1: pass1 y3→2; pass2 y2→ deleted? new_y = -1 < top-1=0 → deleted.
    // So the map may be smaller; whatever remains must have in-region rows.
    for p in storage.placements.values() {
        let y = unsafe { (*p.pin).y };
        assert!((1..=4).contains(&y), "restored pin out of region: {y}");
    }

    t.deinit();
    println!("ported: ok");
}

/// DOCUMENTED-UB foil: a RECORDED pin is untracked (freed) between begin and
/// end — the storage-mutation-in-the-window contract violation — and the
/// guard's drop writes through the dangling pointer. Miri must flag it.
fn ub_end_after_untrack() {
    let mut t = Terminal::new();
    add_placement(&mut t, 1, 0, 3);

    let guard = t.kitty_scroll_margins_begin(-1);

    // CONTRACT VIOLATION: storage mutation between begin and end frees the
    // recorded pin's pool allocation.
    let screen = t.screens.active;
    let pin = unsafe { (*screen).kitty_images.placements.get(&1).unwrap().pin };
    unsafe { (*screen).pages.untrack_pin(pin) };
    unsafe { (*screen).kitty_images.placements.remove(&1) };

    t.row_op(-1);
    guard.end(); // writes through the freed pin → UAF, Miri reports it.

    t.deinit();
    println!("ub_end_after_untrack: reached end without a Miri report (BAD)");
}

fn main() {
    let candidate = std::env::args().nth(1).unwrap_or_default();
    match candidate.as_str() {
        "ported" => ported(),
        "ub_end_after_untrack" => ub_end_after_untrack(),
        other => {
            eprintln!("unknown candidate: {other:?} (want ported|ub_end_after_untrack)");
            std::process::exit(2);
        }
    }
}
