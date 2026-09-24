//! This is the render state that is given to a renderer.
//!
//! PORTS: ghostty src/renderer/State.zig
//!
//! PORT STATUS: done — both `test{}` blocks ported.

// const std = @import("std");
// const assert = std.debug.assert;
// const Allocator = std.mem.Allocator;
// const Inspector = @import("../inspector/main.zig").Inspector;
// const terminalpkg = @import("../terminal/main.zig");
// const inputpkg = @import("../input.zig");
// const renderer = @import("../renderer.zig");
//
// PORT NOTE: `renderer` (`../renderer.zig`) is imported by the reference but not
// referenced by any declaration in this file, so no `use` is bound for it.
// PORT NOTE (D4, oracle d34b54e9b): the handoff atomics map `std.atomic.Value(u32)`
// -> `AtomicU32` and `.monotonic` -> `Ordering::Relaxed`; `StdMutex`/`Condvar` are
// the `std.Thread.Futex` substitute (see the `handoff_park` field).
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Condvar, Mutex as StdMutex};
use std::time::Duration;

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

    /// The number of threads currently waiting to acquire `mutex` via
    /// [`State::lock_demand`]. This is not protected by the mutex; it is read by
    /// hot lock/unlock loops (the IO parse thread) in [`State::yield_to_demand`]
    /// to decide whether to hand the mutex off before relocking it.
    // (reference default: `= .init(0)`)
    pub demand: AtomicU32,

    /// Handoff generation counter. Incremented (with a wake) by
    /// [`State::unlock_demand`] after a demanding waiter releases the mutex, so
    /// that [`State::yield_to_demand`] knows the waiter had its turn.
    // (reference default: `= .init(0)`)
    pub handoff_gen: AtomicU32,

    /// PORT NOTE (Rust-only field — the `std.Thread.Futex` substitute): the
    /// reference parks and wakes directly on the `handoff_gen` word with
    /// `std.Thread.Futex.wake`/`timedWait`. Rust's std futex is internal (not
    /// public API), `rustty-zigstd` has no futex wrapper (only the
    /// `Condvar`-backed `thread::Mutex`), and macOS' `os_sync_wait_on_address`
    /// is macOS-14+/`libc`-unexposed — so the park/wake pair is emulated with a
    /// `Mutex`+`Condvar` gate. `handoff_gen` STAYS a plain relaxed atomic so the
    /// parse loop's fast path is still a single relaxed load per batch (the
    /// oracle's stated cost); the gate is touched only on the slow paths
    /// (`unlock_demand`'s wake and `yield_to_demand`'s park). See
    /// [`State::yield_to_demand`] for the lock-ordering argument.
    ///
    /// Public only because `State` is built by struct literal from other crates
    /// (`rustty-core`'s `Surface.init`, `rustty-termio`'s harnesses); construct
    /// it as `HandoffPark::default()`.
    pub handoff_park: HandoffPark,
}

/// The park/wake primitive behind [`State::yield_to_demand`] — see the
/// `handoff_park` field's PORT NOTE. Stands in for `std.Thread.Futex` on the
/// `handoff_gen` word.
#[derive(Default)]
pub struct HandoffPark {
    /// Held only across the generation snapshot + park (and momentarily by the
    /// waker). Never held while `State.mutex` is held — this is a LEAF lock.
    gate: StdMutex<()>,

    /// Signaled by `unlock_demand` after bumping `handoff_gen`.
    wake: Condvar,
}

impl HandoffPark {
    /// Acquire `gate`, IGNORING std poisoning: the gate guards no data at all
    /// (`Mutex<()>`), so a panic elsewhere cannot leave it in a bad state, and
    /// the reference's futex has no poisoning concept.
    #[inline]
    fn gate(&self) -> std::sync::MutexGuard<'_, ()> {
        match self.gate.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
}

/// How long [`State::yield_to_demand`] sleeps waiting for a demanding waiter to
/// take its turn before giving up. This bounds how long the IO parse
/// thread can stall if a wake is lost or the waiter is descheduled; a
/// demanding critical section (the renderer's frame snapshot) is
/// microseconds, so one millisecond is generous.
//
// Zig: `const handoff_timeout_ns = 1 * std.time.ns_per_ms;`
const HANDOFF_TIMEOUT: Duration = Duration::from_millis(1);

impl State {
    /// Acquire `mutex` while signaling demand for it. Use this instead of
    /// locking the mutex directly on threads that must not be starved by
    /// a hot lock/unlock loop (the renderer's frame snapshot). Must be
    /// released with `unlock_demand`; releasing with `mutex.unlock` keeps
    /// the data safe but makes parked `yield_to_demand` callers wait out
    /// their full timeout.
    ///
    /// Both `std.Thread.Mutex` and os_unfair_lock are unfair: a running
    /// thread that unlocks and immediately relocks beats a sleeping
    /// waiter every time, because the waiter must first be woken and
    /// scheduled. Under sustained pty output the IO parse thread is
    /// exactly such a loop, so without this signal the renderer can
    /// starve for as long as the output lasts.
    ///
    /// See [`State::unlock_demand`] and [`State::yield_to_demand`].
    ///
    /// PORT NOTE: rustty's `mutex` field is a RAW pointer to the guard-less
    /// `rustty_zigstd::thread::Mutex` (the core-tier mutex model), so — unlike
    /// the reference's `self.mutex.lock()` — dereferencing it is an unsafe
    /// operation and this fn is `unsafe`, exactly like every other `state.mutex`
    /// use in the port. It is NOT unsafe for any reason of its own.
    ///
    /// # Safety
    /// `self.mutex` must point at a live [`rustty_zigstd::thread::Mutex`], and
    /// the caller must not already hold it (recursive locking deadlocks).
    pub unsafe fn lock_demand(&self) {
        // _ = self.demand.fetchAdd(1, .monotonic);
        self.demand.fetch_add(1, Ordering::Relaxed);
        // self.mutex.lock();
        unsafe { (*self.mutex).lock() };
        // const prev = self.demand.fetchSub(1, .monotonic);
        let prev = self.demand.fetch_sub(1, Ordering::Relaxed);
        // assert(prev > 0);
        assert!(prev > 0);
    }

    /// Release `mutex` acquired via [`State::lock_demand`] and notify hot loops
    /// parked in [`State::yield_to_demand`] that the demanding waiter had its
    /// turn.
    ///
    /// # Safety
    /// `self.mutex` must point at a live [`rustty_zigstd::thread::Mutex`] that
    /// was acquired via [`State::lock_demand`] (unlocking an unlocked
    /// guard-less mutex is UB in Zig / a debug assertion here).
    pub unsafe fn unlock_demand(&self) {
        // self.mutex.unlock();
        unsafe { (*self.mutex).unlock() };
        // _ = self.handoff_gen.fetchAdd(1, .monotonic);
        self.handoff_gen.fetch_add(1, Ordering::Relaxed);
        // std.Thread.Futex.wake(&self.handoff_gen, 1);
        //
        // The gate is taken (and immediately dropped) BEFORE notifying so a
        // waiter cannot be sitting between its `handoff_gen` snapshot and its
        // park: whoever holds the gate has either not yet snapshotted (and will
        // then read the NEW generation, skipping the park) or is already parked
        // (and will get the notification). This is the lost-wakeup interlock the
        // futex does internally with its bucket lock; the `wake(_, 1)` count of
        // one maps to `notify_one`.
        drop(self.handoff_park.gate());
        self.handoff_park.wake.notify_one();
    }

    /// Called by hot lock/unlock loops between critical sections, with `mutex`
    /// NOT held: if a [`State::lock_demand`] waiter exists, sleep until it has
    /// acquired and released the mutex (or the timeout passes). This is the
    /// handoff that unfair mutexes never do on their own.
    ///
    /// The orderings here are all monotonic because these atomics are a
    /// scheduling heuristic, not a synchronization boundary: the mutex itself
    /// orders the protected data, and the timeout bounds any staleness.
    ///
    /// LOCK ORDERING (rustty-only note, see the `handoff_park` PORT NOTE): the
    /// gate is a LEAF lock. It is acquired only here (with `State.mutex` NOT
    /// held — the documented contract) and in `unlock_demand` AFTER `State.mutex`
    /// has been released. `State.mutex` is never acquired while the gate is
    /// held. There is therefore no lock cycle and no new deadlock edge: the
    /// gate's hold times are a generation load and a `notify_one`.
    pub fn yield_to_demand(&self) {
        // if (self.demand.load(.monotonic) == 0) return;
        if self.demand.load(Ordering::Relaxed) == 0 {
            return;
        }

        // Snapshot the generation before rechecking demand: if the waiter
        // acquires and releases between our check and the wait below, the
        // generation no longer matches and the wait returns immediately.
        //
        // (The gate is held across the snapshot + park so the wake cannot slip
        // in between them — see `unlock_demand`.)
        let gate = self.handoff_park.gate();
        // const gen = self.handoff_gen.load(.monotonic);
        let generation = self.handoff_gen.load(Ordering::Relaxed);
        // if (self.demand.load(.monotonic) == 0) return;
        if self.demand.load(Ordering::Relaxed) == 0 {
            return;
        }
        // std.Thread.Futex.timedWait(&self.handoff_gen, gen, handoff_timeout_ns) catch {};
        //
        // `wait_timeout_while` re-checks the predicate on every wake, so a
        // spurious `notify_one` (or a wake meant for another parked yielder)
        // does not end the wait early; the timeout is the outer bound, and a
        // timeout is ignored exactly like the reference's `catch {}`.
        let _ = self
            .handoff_park
            .wake
            .wait_timeout_while(gate, HANDOFF_TIMEOUT, |_| {
                self.handoff_gen.load(Ordering::Relaxed) == generation
            });
    }
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

    /// A `State` shared across test threads.
    ///
    /// `State` holds raw pointers (`mutex`, `terminal`) so it is neither `Send`
    /// nor `Sync`; the handoff protocol under test is nonetheless designed to be
    /// exercised from several threads at once (that is the whole point of it).
    /// This wrapper asserts what the production code already relies on: the
    /// `State` allocation outlives every thread that touches it, and the only
    /// members these tests touch (`mutex`, `demand`, `handoff_gen`,
    /// `handoff_park`) are themselves thread-safe.
    struct SharedState(State);
    unsafe impl Send for SharedState {}
    unsafe impl Sync for SharedState {}

    /// Build a `State` whose `mutex` is live and whose `terminal` is a dangling
    /// (never-dereferenced) pointer: the handoff protocol touches only `mutex`
    /// and the two atomics, never `terminal`. Forming a dangling raw pointer is
    /// well defined — only dereferencing one would not be.
    fn handoff_state(mutex: &mut rustty_zigstd::thread::Mutex) -> SharedState {
        SharedState(State {
            mutex,
            terminal: std::ptr::NonNull::<Terminal>::dangling().as_ptr(),
            inspector: None,
            inspector_ptr: None,
            preedit: None,
            mouse: Mouse::default(),
            demand: std::sync::atomic::AtomicU32::new(0),
            handoff_gen: std::sync::atomic::AtomicU32::new(0),
            handoff_park: HandoffPark::default(),
        })
    }

    /// PORTS: oracle d34b54e9b — a hot lock/unlock loop must hand the mutex to a
    /// demanding waiter instead of starving it.
    ///
    /// HONESTY NOTE (measured, not assumed): this test PASSES even with the body of
    /// `yield_to_demand` deleted — verified by neutering it and re-running. A
    /// three-instruction lock/unlock loop on an idle machine does not reproduce
    /// starvation, and rustty's `State.mutex` is the `Condvar`-backed
    /// `rustty_zigstd::thread::Mutex` whose `unlock` already does a `notify_one`.
    /// So this test proves the protocol works END TO END (demand accounting stays
    /// balanced, a demanding lock/unlock cycle completes, the generation is bumped
    /// exactly once) and guards those against regression — it does NOT prove the
    /// fairness property. The mechanism itself is proven deterministically by
    /// `renderer_state_yield_to_demand_parks_while_demand_exists` above.
    ///
    /// PORT NOTE (test shape): the plan's sketch used `State::new_for_test()` and
    /// an RAII `StateGuard`; neither exists — rustty's `State.mutex` is the
    /// GUARD-LESS `rustty_zigstd::thread::Mutex` (the core-tier mutex model), so
    /// `lock_demand`/`unlock_demand` are the Zig-shaped unpaired calls and the
    /// state is built by struct literal as everywhere else in the port.
    #[test]
    fn renderer_state_yield_to_demand_hands_off_to_a_waiter() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;

        let mut mutex = rustty_zigstd::thread::Mutex::new();
        let state = Arc::new(handoff_state(&mut mutex));
        let waiter_ran = Arc::new(AtomicBool::new(false));
        let stop = Arc::new(AtomicBool::new(false));

        // Renderer-like thread: takes the lock with demand exactly once.
        let s = state.clone();
        let w = waiter_ran.clone();
        let (tx, rx) = std::sync::mpsc::channel::<()>();
        let renderer = std::thread::spawn(move || {
            unsafe { s.0.lock_demand() };
            w.store(true, Ordering::Relaxed);
            unsafe { s.0.unlock_demand() };
            let _ = tx.send(());
        });

        // Parse-loop-like thread: relocks in a tight loop with a yield between.
        let s = state.clone();
        let st = stop.clone();
        let parser = std::thread::spawn(move || {
            while !st.load(Ordering::Relaxed) {
                unsafe { (*s.0.mutex).lock() };
                unsafe { (*s.0.mutex).unlock() };
                s.0.yield_to_demand();
            }
        });

        // Explicit bound so a failure is a failure rather than a hang.
        let handed_off = rx.recv_timeout(std::time::Duration::from_secs(5)).is_ok();
        stop.store(true, Ordering::Relaxed);
        renderer.join().unwrap();
        parser.join().unwrap();
        assert!(handed_off, "renderer thread was starved by the parse loop");
        assert!(waiter_ran.load(Ordering::Relaxed));

        // The handoff was observed by `unlock_demand` bumping the generation, and
        // the waiter count is balanced again.
        assert_eq!(0, state.0.demand.load(Ordering::Relaxed));
        assert_eq!(1, state.0.handoff_gen.load(Ordering::Relaxed));
    }

    /// rustty-only: the fast path. With no demanding waiter, `yield_to_demand`
    /// is a single relaxed load and must not sleep (the oracle's stated cost for
    /// the parse loop when the renderer is not waiting).
    #[test]
    fn renderer_state_yield_to_demand_is_free_without_demand() {
        let mut mutex = rustty_zigstd::thread::Mutex::new();
        let state = handoff_state(&mut mutex);

        let start = std::time::Instant::now();
        for _ in 0..10_000 {
            state.0.yield_to_demand();
        }
        // 10k iterations of HANDOFF_TIMEOUT would be 10 seconds; a relaxed load
        // loop is microseconds. A generous bound keeps this non-flaky while
        // still failing loudly if the fast path ever starts parking.
        assert!(
            start.elapsed() < std::time::Duration::from_millis(500),
            "yield_to_demand parked with no demand ({:?})",
            start.elapsed()
        );
        assert_eq!(
            0,
            state
                .0
                .handoff_gen
                .load(std::sync::atomic::Ordering::Relaxed)
        );
    }

    /// rustty-only, and the load-bearing test for this task: THE MECHANISM. While a
    /// demanding waiter exists and no handoff has happened, `yield_to_demand` must
    /// keep the hot loop OFF the lock — i.e. it must park, for up to (and here, all
    /// of) `HANDOFF_TIMEOUT` — rather than falling straight through into the next
    /// relock. This is deterministic and it FAILS (returns instantly) if the body of
    /// `yield_to_demand` is removed, which the multi-threaded starvation test below
    /// does NOT (see its note).
    #[test]
    fn renderer_state_yield_to_demand_parks_while_demand_exists() {
        use std::sync::atomic::Ordering;

        let mut mutex = rustty_zigstd::thread::Mutex::new();
        let state = handoff_state(&mut mutex);

        // A waiter exists and never takes its turn, so no generation bump ever
        // arrives: the park must run out its full timeout.
        state.0.demand.fetch_add(1, Ordering::Relaxed);

        let start = std::time::Instant::now();
        state.0.yield_to_demand();
        let elapsed = start.elapsed();

        assert!(
            elapsed >= HANDOFF_TIMEOUT / 2,
            "yield_to_demand did not park with a waiter present ({elapsed:?})"
        );
        // And the timeout really does bound it: a lost wake cannot stall IO
        // forever. (Generous upper bound — this is a scheduler-dependent sleep.)
        assert!(
            elapsed < std::time::Duration::from_millis(250),
            "yield_to_demand overran its timeout ({elapsed:?})"
        );
        assert_eq!(0, state.0.handoff_gen.load(Ordering::Relaxed));
    }

    /// rustty-only: `yield_to_demand` observes a handoff that happens while it is
    /// parked, and returns promptly (well inside `HANDOFF_TIMEOUT`-bounded
    /// pessimism) rather than waiting out its timeout. This is the deterministic
    /// half of the fairness property: the wake path is exercised end-to-end.
    #[test]
    fn renderer_state_unlock_demand_wakes_a_parked_yield() {
        use std::sync::atomic::Ordering;
        use std::sync::Arc;

        let mut mutex = rustty_zigstd::thread::Mutex::new();
        let state = Arc::new(handoff_state(&mut mutex));

        // Fake a waiter: bump `demand` without ever taking the mutex, so the
        // yielding thread is guaranteed to park.
        state.0.demand.fetch_add(1, Ordering::Relaxed);

        let s = state.clone();
        let waker = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_micros(200));
            s.0.demand.fetch_sub(1, Ordering::Relaxed);
            unsafe { (*s.0.mutex).lock() };
            unsafe { s.0.unlock_demand() };
        });

        state.0.yield_to_demand();
        waker.join().unwrap();
        assert_eq!(1, state.0.handoff_gen.load(Ordering::Relaxed));
    }

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
