//! Isolated, non-FFI Miri harness that PROVES the `rt_surface` self-referential
//! pin (PREPORT_SWIFT_APP.md invariant I2 / risk R-E — "the sharpest UB trap").
//!
//! Faithful model of the `crates/rustty-apprt-c` -> `crates/rustty-apprt`
//! `ghostty_surface_new` allocation:
//!
//!   * The embedded container `Surface` holds a core `CoreSurface` BY VALUE
//!     (`embedded.rs:889 pub core_surface: CoreSurface`).
//!   * Core `Surface::init` writes `core_surface.rt_surface = <ptr to the CONTAINER>`
//!     (`surface.rs:1953 std::ptr::write(&raw mut (*self_).rt_surface, rt_surface)`,
//!     where `rt_surface == surface`, the embedded container ptr threaded through
//!     `embedded.rs:1185`). So after init: `container.core_surface.rt_surface ==
//!     &container` — a SELF-REFERENTIAL back-pointer.
//!   * Every subsequent action callback ships `v.rt_surface` (the container addr) to
//!     Swift and derefs it (`performPreAction`'s `set_title` reads/writes through
//!     `surface.rt_surface_ptr()`, embedded.rs:530). If the container is EVER moved
//!     after init, that back-pointer dangles -> UB (R-E).
//!
//! The SHIPPED allocation defends the pin by NEVER holding a movable `Box<Surface>`
//! after init: `newSurface` does `Box::into_raw(Box::new(MaybeUninit::uninit()))`,
//! lowers immediately to a raw `*mut Surface`, inits in place, and hands the raw ptr
//! to the C host; `closeSurface` frees via `Box::from_raw(surface as *mut
//! ManuallyDrop<Surface>)` (dealloc WITHOUT moving the already-deinit'd Surface). The
//! heap address is address-stable for the surface's whole life.
//!
//! Each candidate is one executed path (Miri only interprets executed code), selected
//! by argv[1]. Pass criterion: `pin_shipped` (boxed + no-move) CLEAN under BOTH
//! Stacked Borrows and Tree Borrows; the NEGATIVE `pin_move*` candidates (move the
//! container after init, then follow the stale back-pointer) flagged UB.
#![allow(dead_code)]

use std::mem::{ManuallyDrop, MaybeUninit};

// ---- CoreSurface analogue (`rustty_core::surface::Surface<A>`) ----------------
// Only the load-bearing field is modelled: the self-referential back-pointer plus a
// couple of plain fields so the struct is a realistic (non-ZST) heap object.
struct CoreSurface {
    // rt_surface: *mut A::Surface  (surface.rs:306) — points at the CONTAINER.
    rt_surface: *mut Surface,
    // a couple of stand-in core fields (font_size / renderer handle, etc).
    mailbox_tick: u64,
    live: bool,
}

// ---- embedded container `Surface` (`rustty_apprt::embedded::Surface`) ----------
struct Surface {
    // app: *mut App  (embedded.rs:883) — an opaque up-pointer (separate alloc).
    app: *mut u8,
    // core_surface: CoreSurface  (embedded.rs:889) — held BY VALUE (the self-alias
    // target is INSIDE this container).
    core_surface: CoreSurface,
    // title: ?[:0]const u8  (embedded.rs:906) — the field `performPreAction`'s
    // set_title arm writes THROUGH `rt_surface` (the back-pointer round-trip).
    title: u64,
}

impl Surface {
    /// Faithful placement-init: writes every field in place through the out-pointer
    /// (mirrors `embedded::Surface::init` + `CoreSurface::init`'s `ptr::write`
    /// idiom), INCLUDING the self-referential `core_surface.rt_surface = surface`.
    /// After this returns, `(*surface).core_surface.rt_surface == surface`.
    unsafe fn init(surface: *mut Surface, app: *mut u8) {
        unsafe {
            std::ptr::write(&raw mut (*surface).app, app);
            // core_surface written in place (surface.rs `ptr::write` field-by-field).
            std::ptr::write(&raw mut (*surface).core_surface.mailbox_tick, 0);
            std::ptr::write(&raw mut (*surface).core_surface.live, true);
            // THE SELF-ALIAS: rt_surface = &*surface (the container's own address).
            std::ptr::write(&raw mut (*surface).core_surface.rt_surface, surface);
            std::ptr::write(&raw mut (*surface).title, 0);
        }
    }

    /// Models an action callback that reaches the embedded backend THROUGH the
    /// `rt_surface` back-pointer (`performPreAction` set_title:
    /// `let rt = surface.rt_surface_ptr() as *mut Surface; (*rt).title = ...`). This
    /// is the deref that dangles if the container was moved after init.
    unsafe fn set_title_via_backpointer(surface: *mut Surface, v: u64) {
        unsafe {
            // read the stored back-pointer out of core_surface (a Copy raw field
            // read — narrow, no whole-Surface reborrow), then deref it.
            let rt: *mut Surface = (*surface).core_surface.rt_surface;
            (*rt).title = v; // set_title through rt_surface
            (*rt).core_surface.mailbox_tick = (*rt).core_surface.mailbox_tick.wrapping_add(1);
        }
    }

    unsafe fn read_title_via_backpointer(surface: *const Surface) -> u64 {
        unsafe {
            let rt: *const Surface = (*surface).core_surface.rt_surface;
            (*rt).title
        }
    }
}

// ============ CANDIDATE: pin_shipped (expect CLEAN under SB + TB) ============
// The SHIPPED `ghostty_surface_new` / `ghostty_surface_free` shape: box an
// uninitialized slot, lower to a raw ptr IMMEDIATELY, init in place (self-alias
// written to the final heap address), drive many action callbacks through the
// back-pointer, then free via ManuallyDrop WITHOUT moving. No movable `Box<Surface>`
// ever exists after init, so the container is address-stable and the back-pointer
// never dangles.
fn pin_shipped() {
    // let uninit = Box::into_raw(Box::new(MaybeUninit::uninit()));  (embedded.rs:467)
    let uninit: *mut MaybeUninit<Surface> = Box::into_raw(Box::new(MaybeUninit::uninit()));
    let surface: *mut Surface = uninit.cast();

    let app: *mut u8 = Box::into_raw(Box::new(0xABu8));

    unsafe {
        Surface::init(surface, app);

        // The container is NEVER moved. Drive a sequence of action callbacks that
        // each round-trip through rt_surface (the exact I2 usage the host performs
        // over the surface's whole life).
        for i in 0..8u64 {
            Surface::set_title_via_backpointer(surface, i);
            let t = Surface::read_title_via_backpointer(surface);
            assert_eq!(t, i, "back-pointer round-trip must observe the just-written title");
            // sanity: the back-pointer still resolves to the SAME container address.
            assert_eq!(
                (*surface).core_surface.rt_surface, surface,
                "rt_surface must stay pinned to the container across callbacks"
            );
        }

        // closeSurface: free the allocation WITHOUT moving/dropping the Surface
        // (ManuallyDrop free after a logical deinit) — embedded.rs:500.
        (*surface).core_surface.live = false; // deinit stand-in
        drop(Box::from_raw(surface as *mut ManuallyDrop<Surface>));
        drop(Box::from_raw(app));
    }
    println!("pin_shipped: OK (no move; rt_surface stayed pinned)");
}

// ============ NEGATIVE: pin_move (expect UB — SB + TB) ============
// Models the R-E violation: after init writes the self-alias to the container's
// address `p0`, the container VALUE is MOVED to a fresh allocation `p1` and the
// ORIGINAL storage `p0` is freed. The moved copy still carries the OLD back-pointer
// (`rt_surface == p0`), now DANGLING, so the next action callback's deref is a
// use-after-free — flagged AT THE DEREF (not at cleanup).
//
// NB: a `Box<Surface>` MOVE alone (`let b2 = b1;`) relocates only the pointer, not
// the pointee — that would NOT break the pin. To actually move the SURFACE you must
// relocate its bytes to a new allocation, which is exactly what a naive
// "reallocate / return-by-value" refactor of `newSurface` would do. The raw
// alloc/`ptr::read`/free sequence below reproduces that with a SINGLE free of each
// allocation (no incidental double-free to mask the intended UAF).
fn pin_move() {
    let app: *mut u8 = Box::into_raw(Box::new(0xABu8));

    // Box an uninit slot, lower to raw, init in place (self-alias -> p0).
    let uninit0: *mut MaybeUninit<Surface> = Box::into_raw(Box::new(MaybeUninit::uninit()));
    let p0: *mut Surface = uninit0.cast();
    unsafe { Surface::init(p0, app) };
    unsafe { Surface::set_title_via_backpointer(p0, 5) }; // valid: still pinned at p0

    unsafe {
        // MOVE: bit-copy the Surface value OUT of p0 to a fresh allocation p1, then
        // FREE the original storage p0. `val.core_surface.rt_surface` is still p0.
        let val: Surface = std::ptr::read(p0);
        drop(Box::from_raw(uninit0)); // free p0's storage (the surface moved away)
        let p1: *mut Surface = Box::into_raw(Box::new(val));

        // Action callback AFTER the move: follows (*p1).core_surface.rt_surface == p0
        // (the FREED original) -> use-after-free. Miri flags THIS deref.
        Surface::set_title_via_backpointer(p1, 7); // UB: derefs the freed p0

        drop(Box::from_raw(p1 as *mut ManuallyDrop<Surface>));
        drop(Box::from_raw(app));
    }
    println!("pin_move: (unreachable — Miri should have flagged UB at the rt_surface deref)");
}

// ============ NEGATIVE: pin_move_readonly (expect UB — SB + TB) ============
// Same move hazard reached through a read-only callback path (`getTitle` via
// rt_surface, e.g. surface.rs:2976 `self.rt_surface.getTitle()`), to show the pin
// break is not specific to the write path.
fn pin_move_readonly() {
    let app: *mut u8 = Box::into_raw(Box::new(0xABu8));
    let uninit0: *mut MaybeUninit<Surface> = Box::into_raw(Box::new(MaybeUninit::uninit()));
    let p0: *mut Surface = uninit0.cast();
    unsafe { Surface::init(p0, app) };
    unsafe { Surface::set_title_via_backpointer(p0, 99) }; // valid: still pinned

    unsafe {
        let val: Surface = std::ptr::read(p0);
        drop(Box::from_raw(uninit0)); // free p0's storage
        let p1: *mut Surface = Box::into_raw(Box::new(val));

        let _t = Surface::read_title_via_backpointer(p1); // UB: derefs freed p0

        drop(Box::from_raw(p1 as *mut ManuallyDrop<Surface>));
        drop(Box::from_raw(app));
    }
    println!("pin_move_readonly: (unreachable — Miri should have flagged UB at the rt_surface deref)");
}

fn main() {
    let which = std::env::args().nth(1).unwrap_or_else(|| "pin_shipped".to_string());
    match which.as_str() {
        "pin_shipped" => pin_shipped(),
        "pin_move" => pin_move(),
        "pin_move_readonly" => pin_move_readonly(),
        other => {
            eprintln!("unknown candidate: {other}");
            std::process::exit(2);
        }
    }
}
