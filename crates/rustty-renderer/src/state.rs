//! This is the render state that is given to a renderer.
//!
//! PORTS: ghostty src/renderer/State.zig
//!
//! PORT STATUS: done — both `test{}` blocks ported.

// const std = @import("std");
// const Allocator = std.mem.Allocator;
// const Inspector = @import("../inspector/main.zig").Inspector;
// const terminalpkg = @import("../terminal/main.zig");
// const inputpkg = @import("../input.zig");
// const renderer = @import("../renderer.zig");
//
// PORT NOTE: `renderer` (`../renderer.zig`) is imported by the reference but not
// referenced by any declaration in this file, so no `use` is bound for it.
use rustty_vt::input::key_mods::Mods;
use rustty_vt::point;
use rustty_vt::size::CellCountInt;
use rustty_vt::terminal::Terminal;

/// The RENDERER-owned accessor trait for the inspector→renderer overlay edge
/// (INV-7 / hazard R2), mirroring the [`crate::config::RendererConfig`]
/// precedent (memory: `renderer-config-accessor-trait-acyclic`).
///
/// THE ACYCLIC CRUX (R4/G7): the inspector↔renderer overlay coupling is a real
/// bidirectional data edge — the inspector's checkboxes WRITE
/// `inspector::widgets::renderer::Info.features` and the renderer READS them here
/// (generic.rs, in `update_frame`'s critical section). To keep the renderer from
/// naming any `rustty-inspector` type (which would add a `renderer -> inspector`
/// edge and CYCLE — the inspector is the upper tier), the renderer DEFINES this
/// trait and the inspector IMPLEMENTS it (the `rustty-inspector -> rustty-renderer`
/// edge is the acyclic direction; `check_acyclic.sh` stays green). This is the
/// exact shape `RendererConfig` uses for the config edge: the trait's only return
/// type is renderer-nameable (the renderer-local [`crate::overlay::Feature`]).
///
/// PORT NOTE: this replaces the reference's direct
/// `state.inspector.?.rendererInfo().overlayFeatures(alloc)` call chain
/// (generic.zig:1266 / renderer.zig:25-41). The reference navigates the whole
/// `Inspector` to its `renderer.Info` at CALL time; the port stores a fat pointer
/// to the `Info` itself (see the `Inspector` alias below) and reads through this
/// one-method trait. `overlayFeatures` dupes `Info.features`' values into a fresh
/// owned slice the renderer owns/frees — here a plain owned `Vec` (Global alloc;
/// the reference's `catch &.{}` OOM path is subsumed by panic=abort).
pub trait InspectorOverlay {
    /// Grab the enabled overlay features into a fresh owned `Vec` (INV-7). Faithful
    /// to `renderer.zig`'s `overlayFeatures`: a plain value copy of `Info.features`'
    /// values (the features carry no managed memory).
    fn overlay_features(&self) -> Vec<crate::overlay::Feature>;
}

// PORT NOTE (R2 ACCESSOR-TRAIT SEAM — was DEFERRED): the reference `inspector`
// field points at `inspector/main.zig`'s `Inspector`. The renderer may NEVER name
// a `rustty-inspector` type (R4 acyclicity — a `renderer -> inspector` edge would
// CYCLE), so the pointee type is the renderer-owned trait object
// [`dyn InspectorOverlay`] (the acyclic accessor-trait, mirroring the
// `RendererConfig` precedent). This SUPERSEDES the earlier uninhabited
// `enum Inspector {}` placeholder: the field is now a live `*mut dyn` fat pointer,
// through which `update_frame` reads overlay features (generic.rs). Core's
// `activateInspector` (Task 6) will coerce the real
// `rustty_inspector::widgets::renderer::Info` (which impls `InspectorOverlay`) into
// this fat pointer — the `inspector -> renderer` edge, never the reverse. The
// field's optionality and pointer-ness are preserved.
pub type Inspector = dyn InspectorOverlay;

/// The mutex that must be held while reading any of the data in the
/// members of this state. Note that the state itself is NOT protected
/// by the mutex and is NOT thread-safe, only the members values of the
/// state (i.e. the terminal, devmode, etc. values).
//
// PORT NOTE: the reference is a top-level file-struct (`State.zig`); the Rust
// port names it `State`. `mutex: *std.Thread.Mutex` maps to a raw pointer to
// the GUARD-LESS `rustty_zigstd::thread::Mutex` (the Zig-shaped manual
// `lock`/`try_lock`/`unlock` mutex — the core-tier mutex-model decision,
// CORE_BLUEPRINT §3/C0: Surface.zig and the termio writers unlock/relock this
// mutex across function boundaries, patterns a guard-based `std::sync::Mutex`
// cannot express; every lock/unlock pairs verbatim with a reference line). The
// struct is constructed by the caller supplying `mutex`/`terminal`; the
// reference field defaults (`inspector = null`, `preedit = null`,
// `mouse = .{}`) are documented at each field.
pub struct State {
    /// The mutex that must be held while reading any of the data in the
    /// members of this state.
    pub mutex: *mut rustty_zigstd::thread::Mutex,

    /// The terminal data.
    pub terminal: *mut Terminal,

    /// The terminal inspector, if any. This will be null while the inspector
    /// is not active and will be set when it is active.
    // (reference default: `= null`)
    pub inspector: Option<*mut Inspector>,

    /// The SAME inspector as `inspector` above, but as an opaque untyped raw
    /// pointer rather than the narrow `*mut dyn InspectorOverlay` fat pointer.
    ///
    /// PORT NOTE (R2/R4 acyclic seam, termio `recordPtyRead` — INV-8): the
    /// reference's `renderer_state.inspector` is the WHOLE `*Inspector`
    /// (Surface.zig `self.renderer_state.inspector = self.inspector`), read
    /// both by the renderer (`rendererInfo().overlayFeatures()`) and by termio
    /// (`insp.recordPtyRead(...)`, termio.zig). The port's `inspector` field
    /// above is intentionally narrowed to `*mut dyn InspectorOverlay` — a fat
    /// pointer to just the `Inspector.gui.renderer_info` sub-object — so the
    /// renderer (a LOWER tier than rustty-inspector) never has to name a
    /// `rustty-inspector` type (would cycle: inspector already deps renderer).
    /// That narrowing, however, orphans the OTHER consumer of the same shared
    /// field: termio needs the WHOLE `Inspector` (for its sibling `vt_stream`
    /// field, unreachable through the `renderer_info` sub-object). Rather than
    /// re-widen `inspector` above (which would force the renderer to name
    /// `rustty_inspector::Inspector`, or move the `InspectorOverlay` impl off
    /// `Info` and onto `Inspector`, both bigger changes than this seam needs),
    /// this field carries the SAME `*mut Inspector` core already computed,
    /// but type-erased to `*mut ()` — the renderer crate stores/moves it
    /// without ever naming the pointee type (no new dependency, no cycle).
    /// Only rustty-termio (which already, like rustty-inspector, sits ABOVE
    /// rustty-renderer and now takes a direct `rustty-inspector` dependency —
    /// acyclic: rustty-inspector does not depend on rustty-termio) casts it
    /// back to `*mut rustty_inspector::Inspector` to call `recordPtyRead`.
    /// Set/cleared in lock-step with `inspector` above by core's
    /// `activateInspector`/`deactivateInspector`.
    pub inspector_ptr: Option<*mut ()>,

    /// Dead key state. This will render the current dead key preedit text
    /// over the cursor. This currently only ever renders a single codepoint.
    /// Preedit can in theory be multiple codepoints long but that is left as
    /// a future exercise.
    // (reference default: `= null`)
    pub preedit: Option<Preedit>,

    /// Mouse state. This only contains state relevant to what renderers
    /// need about the mouse.
    // (reference default: `= .{}`)
    pub mouse: Mouse,
}

// PORT NOTE (GPU R-d Chunk 3c §B.3): `#[derive(Clone, Copy)]` added beside the
// reference's implicit trivially-copyable `Mouse` so `updateFrame`'s `Critical`
// snapshot can do `critical.mouse = state.mouse` (:1275) by value. Sound POD glue:
// `Coordinate` is Copy and `Mods` is Copy+Default, so both fields are Copy.
#[derive(Clone, Copy, Default)]
pub struct Mouse {
    /// The point on the viewport where the mouse currently is. We use
    /// viewport points to avoid the complexity of mapping the mouse to
    /// the renderer state.
    pub point: Option<point::Coordinate>,

    /// The mods that are currently active for the last mouse event.
    /// This could really just be mods in general and we probably will
    /// move it out of mouse state at some point.
    pub mods: Mods,
}

// `mouse: Mouse = .{}` — the reference defaults both fields (`point = null`,
// `mods = .{}`); `#[derive(Default)]` reproduces that exactly (`point` -> `None`,
// `mods` -> `Mods::default()`).

/// The pre-edit state. See Surface.preeditCallback for more information.
pub struct Preedit {
    /// The codepoints to render as preedit text.
    //
    // PORT NOTE: the reference is `[]const Codepoint = &.{}` (a slice, dup'd into
    // an allocator by `clone`). The Rust port owns it as a `Vec<Codepoint>`
    // (default empty), freed on `Drop`.
    pub codepoints: Vec<Codepoint>,
}

/// A single codepoint to render as preedit text.
#[derive(Clone, Copy, Default)]
pub struct Codepoint {
    pub codepoint: u21,
    // wide: bool = false,
    pub wide: bool,
}

// PORT NOTE: `u21` is not a Rust primitive; the ported terminal uses
// `rustty_vt::page::u21` (= `u32`). Aliased locally to read against the reference.
use rustty_vt::page::u21;

/// The start/end x position of the preedit text along with any codepoint
/// offset necessary to fit the preedit into the available space.
//
// PORT NOTE: the reference `range` returns an anonymous struct; Rust names it.
pub struct PreeditRange {
    pub start: CellCountInt,
    pub end: CellCountInt,
    pub cp_offset: usize,
}

impl Preedit {
    /// Deinit this preedit that was cre
    //
    // PORT NOTE: the reference frees `codepoints` via the allocator; the Rust
    // `Vec` frees on `Drop`. Kept for API parity.
    pub fn deinit(&self) {
        // alloc.free(self.codepoints);
    }

    /// Allocate a copy of this preedit in the given allocator..
    //
    // PORT NOTE: the reference `dupe`s into `alloc` and returns `!Preedit`; the
    // Rust port clones the `Vec` with the global allocator (infallible here).
    // PARITY: named `clone` after the reference `Preedit.clone`; not the
    // `std::clone::Clone::clone` trait method.
    #[allow(clippy::should_implement_trait)]
    pub fn clone(&self) -> Preedit {
        Preedit {
            codepoints: self.codepoints.clone(),
        }
    }

    /// The width in cells of all codepoints in the preedit.
    pub fn width(&self) -> usize {
        let mut result: usize = 0;
        for cp in self.codepoints.iter() {
            result += if cp.wide { 2 } else { 1 };
        }

        result
    }

    /// Range returns the start and end x position of the preedit text
    /// along with any codepoint offset necessary to fit the preedit
    /// into the available space.
    pub fn range(&self, start: CellCountInt, max: CellCountInt) -> PreeditRange {
        // If our width is greater than the number of cells we have
        // then we need to adjust our codepoint start to a point where
        // our width would be less than the number of cells we have.
        let (w, cp_offset) = {
            // max is inclusive, so we need to add 1 to it.
            let max_width = max - start + 1;

            // Rebuild our width in reverse order. This is because we want
            // to offset by the end cells, not the start cells (if we have to).
            let mut w: CellCountInt = 0;
            let mut result: Option<(CellCountInt, usize)> = None;
            for i in 0..self.codepoints.len() {
                let reverse_i = self.codepoints.len() - i - 1;
                let cp = self.codepoints[reverse_i];
                w += if cp.wide { 2 } else { 1 };
                if w > max_width {
                    result = Some((w, reverse_i));
                    break;
                }
            }

            // Width fit in the max width so no offset necessary.
            result.unwrap_or((w, 0))
        };

        // If our preedit goes off the end of the screen, we adjust it so
        // that it shifts left.
        let end = if w > 0 { start + (w - 1) } else { start };
        // PARITY: kept as the reference's `if (end > max) end - max else 0`
        // conditional rather than a saturating-sub helper.
        #[allow(clippy::implicit_saturating_sub)]
        let start_offset = if end > max { end - max } else { 0 };
        PreeditRange {
            start: start.saturating_sub(start_offset),
            end: end.saturating_sub(start_offset),
            cp_offset,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_HANGUL_GA: u21 = 0xAC00; // U+AC00 HANGUL SYLLABLE GA

    // test "preedit range covers exact cell width"
    #[test]
    fn preedit_range_covers_exact_cell_width() {
        {
            let p: Preedit = Preedit {
                codepoints: vec![Codepoint {
                    codepoint: 'a' as u21,
                    ..Default::default()
                }],
            };
            let range = p.range(2, 9);
            assert_eq!(2 as CellCountInt, range.start);
            assert_eq!(2 as CellCountInt, range.end);
            assert_eq!(0usize, range.cp_offset);
        }

        {
            let p: Preedit = Preedit {
                codepoints: vec![Codepoint {
                    codepoint: TEST_HANGUL_GA,
                    wide: true,
                }],
            };
            let range = p.range(2, 9);
            assert_eq!(2 as CellCountInt, range.start);
            assert_eq!(3 as CellCountInt, range.end);
            assert_eq!(0usize, range.cp_offset);
        }
    }

    // test "preedit range shifts left at right edge"
    #[test]
    fn preedit_range_shifts_left_at_right_edge() {
        let p: Preedit = Preedit {
            codepoints: vec![Codepoint {
                codepoint: TEST_HANGUL_GA,
                wide: true,
            }],
        };
        let range = p.range(9, 9);
        assert_eq!(8 as CellCountInt, range.start);
        assert_eq!(9 as CellCountInt, range.end);
        assert_eq!(0usize, range.cp_offset);
    }
}
