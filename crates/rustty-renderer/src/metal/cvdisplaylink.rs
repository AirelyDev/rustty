//! cvdisplaylink.rs — faithful port of ghostty `pkg/macos/video/display_link.zig`
//! (the CoreVideo `CVDisplayLink` loop driver). macOS-only (linked via
//! `crate::metal`, itself `#[cfg(target_os = "macos")]`).
//!
//! R1: the reference `DisplayLink = opaque {}` with methods
//! `createWithActiveCGDisplays`/`release`/`start`/`stop`/`isRunning`/
//! `setCurrentCGDisplay`/`setOutputCallback` is ported 1:1. The reference
//! `setOutputCallback` is a comptime-generic over `(Userdata, callbackFn)`;
//! it is instantiated in exactly ONE place — `generic.zig:loopEnter` with
//! `(xev.Async, &displayLinkCallback, &thr.draw_now)` — so the monomorphic port
//! collapses it to a single concrete `set_output_callback(&mut self, Async)`
//! that installs a fixed `extern "C"` trampoline running the fixed
//! `displayLinkCallback` body (notify the `Async`). This mirrors how the rest of
//! this port collapses the comptime `GenericRenderer` hierarchy onto the Metal leaf.

use core::ffi::c_void;
use core::ptr::NonNull;
use rustty_xev::Async;

// const log = std.log.scoped(.generic_renderer);   (generic.zig:46)
//
// This file's ONE diagnostic is `generic.zig:991`'s `log.err`, whose body is
// inlined here (`output_callback` — a CoreVideo `extern "C"` callback cannot be a
// `Self` method), so it keeps the `generic_renderer` scope of the function it
// ports, NOT a cvdisplaylink-local one. DECISION-logging.md §6r call shape 3.
const LOG_SCOPE: &str = "generic_renderer";

// ---------------------------------------------------------------------------
// The CoreVideo C symbols (exact signatures — from `<CoreVideo/CVDisplayLink.h>`).
// Framework-linked via `#[link(name = "CoreVideo", kind = "framework")]` on the
// `extern "C"` block. This mirrors the reference `pkg/macos/video/c.zig`, a raw
// `@cImport("<CoreVideo/CoreVideo.h>")` — the Zig wrapper calls the bare C
// symbols; `extern "C"` is the 1:1 mirror.
// ---------------------------------------------------------------------------

/// Opaque `CVDisplayLinkRef` (reference `pub const DisplayLink = opaque {}`).
#[repr(C)]
pub struct CVDisplayLink {
    _private: [u8; 0],
}
pub type CVDisplayLinkRef = *mut CVDisplayLink;

/// `CVReturn` (`int32_t`); success is `kCVReturnSuccess == 0`.
pub type CVReturn = i32;
#[allow(non_upper_case_globals)] // C-name fidelity (reference `c.kCVReturnSuccess`).
pub const kCVReturnSuccess: CVReturn = 0;

/// `CGDirectDisplayID` (`uint32_t`) — the id `setCurrentCGDisplay`/`setMacOSDisplayID` take.
pub type CGDirectDisplayID = u32;

/// `CVOptionFlags` (`uint64_t`).
pub type CVOptionFlags = u64;

/// `CVTimeStamp` — the reference callback names `*const c.CVTimeStamp` for `inNow`
/// / `inOutputTime` but discards both (`_ = inNow; _ = inOutputTime;`). We bind
/// them as opaque `*const c_void` because they are never dereferenced (faithful:
/// the Zig wrapper likewise throws these away — see its doc comment "this purposely
/// throws away a ton of arguments I didn't need"). Keeping them opaque avoids
/// transcribing the 7-field struct that no code reads.
pub type CVTimeStampRef = *const c_void;

/// `CVDisplayLinkOutputCallback`:
///   CVReturn (*)(CVDisplayLinkRef, const CVTimeStamp*, const CVTimeStamp*,
///                CVOptionFlags, CVOptionFlags*, void*)
pub type CVDisplayLinkOutputCallback = unsafe extern "C" fn(
    display_link: CVDisplayLinkRef,
    in_now: CVTimeStampRef,
    in_output_time: CVTimeStampRef,
    flags_in: CVOptionFlags,
    flags_out: *mut CVOptionFlags,
    user_info: *mut c_void,
) -> CVReturn;

// Note: `CVDisplayLinkSetCurrentCGDisplayFromMetalDevice` (mentioned in the plan
// brief) is NOT in the reference `display_link.zig` wrapper — the reference only
// ever calls `setCurrentCGDisplay(display_id)` (from `setMacOSDisplayID`). Out of
// scope; not bound. Likewise `CVDisplayLinkRetain` is unused (create returns +1;
// `release` is the only refcount op the wrapper performs).
#[link(name = "CoreVideo", kind = "framework")]
extern "C" {
    fn CVDisplayLinkCreateWithActiveCGDisplays(out: *mut CVDisplayLinkRef) -> CVReturn;
    fn CVDisplayLinkSetOutputCallback(
        dl: CVDisplayLinkRef,
        callback: CVDisplayLinkOutputCallback,
        user_info: *mut c_void,
    ) -> CVReturn;
    fn CVDisplayLinkSetCurrentCGDisplay(
        dl: CVDisplayLinkRef,
        display_id: CGDirectDisplayID,
    ) -> CVReturn;
    fn CVDisplayLinkStart(dl: CVDisplayLinkRef) -> CVReturn;
    fn CVDisplayLinkStop(dl: CVDisplayLinkRef) -> CVReturn;
    fn CVDisplayLinkIsRunning(dl: CVDisplayLinkRef) -> u8; // `Boolean`
    fn CVDisplayLinkRelease(dl: CVDisplayLinkRef);
}

/// Reference `DisplayLink.Error = error{ CreationFailed, InvalidOperation }`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// Display link creation failed (e.g. macOS has no active displays —
    /// a locked session). Reference `error.CreationFailed` (a177ba90a).
    CreationFailed,
    /// A CoreVideo call returned non-`kCVReturnSuccess`.
    InvalidOperation,
}

/// Faithful port of `pkg/macos/video/display_link.zig` `DisplayLink`.
///
/// The reference is a bare `opaque` (just the `CVDisplayLinkRef`). This port adds
/// ONE field — `draw_now` — because R4 forbids `rustty-renderer` naming the core
/// `renderer.Thread` that, in the reference, owns the `xev.Async` userdata and
/// provides its stable address (`&thr.draw_now`). We store the `Async` in a
/// `Box` (a SEPARATE heap allocation, set at `set_output_callback` time); the
/// boxed pointee's address is the faithful stand-in for `&thr.draw_now` — and,
/// like the reference's separate `Thread` home, it lives OUTSIDE the `Renderer`
/// `&mut` footprint (see the crate `W2_DESIGN.md` §2.1/§6).
///
/// `Debug` is hand-written (not derived): `xev::Async` does not implement `Debug`,
/// but the enclosing `Renderer` field `display_link: Option<DisplayLink>` needs the
/// bound (the placeholder this replaces was `#[derive(Debug)]`). The impl prints
/// only the opaque link ptr (the `Async` payload is a bare mach port; nothing to show).
pub struct DisplayLink {
    /// The `CVDisplayLinkRef` (reference `@ptrCast(self)`). Non-null: the ctor
    /// rejects a null result exactly as the reference `orelse error.CreationFailed`.
    link: NonNull<CVDisplayLink>,
    /// The userdata the CoreVideo output callback dereferences — the `xev.Async`
    /// the reference stores in `renderer.Thread`. `None` until `set_output_callback`.
    /// `Async` is `Copy`+`Send` (a bare `port: u32`); the callback reads it through
    /// a `*const Async` (never `&mut`). **`Box`ed on purpose**: the heap pointee has
    /// a STABLE address independent of any move of the `DisplayLink`/`Renderer` AND
    /// sits in its OWN allocation, so a `&mut self` reborrow of the `Renderer`
    /// (`loop_enter`/`set_focus`/`deinit`) does NOT cover the bytes the CoreVideo
    /// callback thread aliases — the faithful analogue of the reference keeping
    /// `draw_now` in a separate `renderer.Thread` (never under the renderer's
    /// `&mut`). Written once in `set_output_callback`; never reborrowed `&mut` or
    /// reassigned while the link runs.
    draw_now: Option<Box<Async>>,
}

impl core::fmt::Debug for DisplayLink {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("DisplayLink")
            .field("link", &self.link)
            .field("draw_now", &self.draw_now.is_some())
            .finish()
    }
}

// SAFETY note: `link` is a CoreVideo handle used only from the render thread's
// start/stop/release + the CoreVideo-owned callback thread (which touches only
// `draw_now`, a `Send` `Async`). Mirrors the reference `*DisplayLink` crossing
// into the render thread. (Only add `unsafe impl Send`/`Sync` if a call site needs
// it — the KAT keeps the `DisplayLink` on one thread, so it is unneeded here.)

impl DisplayLink {
    /// Reference `createWithActiveCGDisplays() Error!*DisplayLink`.
    ///     var result: ?*DisplayLink = null;
    ///     if (c.CVDisplayLinkCreateWithActiveCGDisplays(@ptrCast(&result)) != kCVReturnSuccess)
    ///         return error.CreationFailed;
    ///     return result orelse error.CreationFailed;
    pub fn create_with_active_cg_displays() -> Result<DisplayLink, Error> {
        let mut result: CVDisplayLinkRef = core::ptr::null_mut();
        // SAFETY: `&mut result` is a valid out-pointer; CoreVideo writes a +1 ref.
        let rc = unsafe { CVDisplayLinkCreateWithActiveCGDisplays(&mut result) };
        if rc != kCVReturnSuccess {
            return Err(Error::CreationFailed);
        }
        // reference `result orelse error.CreationFailed`
        match NonNull::new(result) {
            Some(link) => Ok(DisplayLink {
                link,
                draw_now: None,
            }),
            None => Err(Error::CreationFailed),
        }
    }

    /// Reference `release(self) void { c.CVDisplayLinkRelease(@ptrCast(self)); }`.
    /// MANUAL (no `Drop`) — faithful to the Zig `opaque` whose release is explicit
    /// (called by `Renderer.deinit`'s teardown arm / the `init` errdefer). A
    /// `Drop` would double-free against those explicit `release()` calls.
    pub fn release(&self) {
        // SAFETY: `link` is a valid +1 CVDisplayLinkRef; caller guarantees the link
        // is stopped and no further method runs (Renderer::deinit / errdefer).
        unsafe { CVDisplayLinkRelease(self.link.as_ptr()) };
    }

    /// Reference `start(self) Error!void`.
    pub fn start(&self) -> Result<(), Error> {
        // SAFETY: `link` is a valid +1 CVDisplayLinkRef.
        let rc = unsafe { CVDisplayLinkStart(self.link.as_ptr()) };
        if rc != kCVReturnSuccess {
            Err(Error::InvalidOperation)
        } else {
            Ok(())
        }
    }

    /// Reference `stop(self) Error!void`.
    pub fn stop(&self) -> Result<(), Error> {
        // SAFETY: `link` is a valid +1 CVDisplayLinkRef.
        let rc = unsafe { CVDisplayLinkStop(self.link.as_ptr()) };
        if rc != kCVReturnSuccess {
            Err(Error::InvalidOperation)
        } else {
            Ok(())
        }
    }

    /// Reference `isRunning(self) bool { return c.CVDisplayLinkIsRunning(...) != 0; }`.
    pub fn is_running(&self) -> bool {
        // SAFETY: `link` is a valid +1 CVDisplayLinkRef.
        unsafe { CVDisplayLinkIsRunning(self.link.as_ptr()) != 0 }
    }

    /// Reference `setCurrentCGDisplay(self, display_id: CGDirectDisplayID) Error!void`.
    pub fn set_current_cg_display(&self, display_id: CGDirectDisplayID) -> Result<(), Error> {
        // SAFETY: `link` is a valid +1 CVDisplayLinkRef.
        let rc = unsafe { CVDisplayLinkSetCurrentCGDisplay(self.link.as_ptr(), display_id) };
        if rc != kCVReturnSuccess {
            Err(Error::InvalidOperation)
        } else {
            Ok(())
        }
    }

    /// Reference `setOutputCallback(self, comptime Userdata, comptime callbackFn, userinfo)`.
    /// MONOMORPHIZED to the single instantiation `(xev.Async, &displayLinkCallback,
    /// &thr.draw_now)`. Stores `draw_now` wrapper-local (its stable boxed address is
    /// the faithful `&thr.draw_now`) and installs the fixed `output_callback` trampoline.
    ///
    /// R1 — the reference comment is preserved verbatim:
    ///   // Note: this purposely throws away a ton of arguments I didn't need.
    ///   // It would be trivial to refactor this into Zig types and properly
    ///   // pass this through.
    pub fn set_output_callback(&mut self, draw_now: Async) -> Result<(), Error> {
        // Store the userdata FIRST (on the heap, via `Box`) so its payload address
        // is stable before CoreVideo captures it. Written exactly once; the boxed
        // `Async` is never moved, reborrowed `&mut`, or reassigned while the link
        // runs (see the field doc / W2_DESIGN.md §6).
        self.draw_now = Some(Box::new(draw_now));
        // The stable `*const Async` CoreVideo hands back to `output_callback`. Taken
        // as a `*const` to the boxed pointee (`as_deref()` -> `*const`) — a separate
        // heap allocation, so NO `Renderer` `&mut` reborrow ever covers these bytes,
        // and NO intervening `&mut Async` is minted over the location the callback
        // thread reads (rust-mut-args-strongly-protected discipline).
        let ud: *const Async = self.draw_now.as_deref().expect("just set");
        // SAFETY: `link` is a valid +1 CVDisplayLinkRef; `output_callback` is a
        // valid `extern "C"` trampoline; `ud` points at the boxed `Async` which
        // outlives the running link (dropped only after `stop()` in deinit/errdefer).
        let rc = unsafe {
            CVDisplayLinkSetOutputCallback(self.link.as_ptr(), output_callback, ud as *mut c_void)
        };
        if rc != kCVReturnSuccess {
            Err(Error::InvalidOperation)
        } else {
            Ok(())
        }
    }
}

/// The fixed monomorphized trampoline = the reference's inner
/// `struct { fn callback(...) callconv(.c) CVReturn { … callbackFn(...); return
/// kCVReturnSuccess; } }`, with `callbackFn` = `displayLinkCallback` inlined.
///   // reference discards inNow/inOutputTime/flagsIn/flagsOut:
///   _ = inNow; _ = inOutputTime; _ = flagsIn; _ = flagsOut;
unsafe extern "C" fn output_callback(
    _display_link: CVDisplayLinkRef,
    _in_now: CVTimeStampRef,
    _in_output_time: CVTimeStampRef,
    _flags_in: CVOptionFlags,
    _flags_out: *mut CVOptionFlags,
    user_info: *mut c_void,
) -> CVReturn {
    // === generic.zig:986-994 `displayLinkCallback` (inlined, R1 body) ===
    //   fn displayLinkCallback(_: *macos.video.DisplayLink, ud: ?*xev.Async) void {
    //       const draw_now = ud orelse return;
    //       draw_now.notify() catch |err| { log.err("error notifying draw_now err={}", .{err}); };
    //   }
    // `ud orelse return`:
    let ud = user_info as *const Async;
    if ud.is_null() {
        return kCVReturnSuccess;
    }
    // W2-T3 vsync proof: count REAL CVDisplayLink ticks (compiled ONLY under
    // `#[cfg(test)]`, so the production trampoline stays branch-free). The KAT's
    // guard-thread fallback `notify()` bypasses THIS function, so `TICKS` rises
    // only on a genuine display-link tick — the on-device gate asserts `TICKS>=1`.
    // Bumped BEFORE `notify()` so the KAT never observes a wakeup whose tick has
    // not yet been recorded.
    #[cfg(test)]
    tests::TICKS.fetch_add(1, core::sync::atomic::Ordering::SeqCst);
    // SAFETY: `ud` is the stable boxed `Async` stored by `set_output_callback`,
    // alive for the running link (separate heap allocation; the link is stopped
    // before the box drops). `Async` is `Copy`; read by value, no aliasing `&mut`.
    // `notify()` only sends to the mach port.
    let draw_now: Async = unsafe { *ud };
    if let Err(err) = draw_now.notify() {
        // reference: log.err("error notifying draw_now err={}", .{err});
        log::error!(target: LOG_SCOPE, "error notifying draw_now err={err:?}");
    }
    kCVReturnSuccess // reference `return c.kCVReturnSuccess;`
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::sync::atomic::{AtomicU32, Ordering};
    use rustty_xev::{Async, CallbackAction, Completion, Loop, RunMode, WaitError};

    /// Real-CVDisplayLink-tick counter, bumped in `output_callback` BEFORE `notify()`
    /// (compiled only under `#[cfg(test)]`). `pub(super)` so the parent module's
    /// `output_callback` can name it as `tests::TICKS`. The 2 s guard thread's
    /// fallback `notify()` does NOT go through `output_callback`, so this counts
    /// ONLY genuine vsync ticks — the load-bearing W2-T3 signal.
    pub(super) static TICKS: AtomicU32 = AtomicU32::new(0);

    // W2-T3 vsync KAT — CVDisplayLink → xev::Async → loop wakeup round-trip.
    // On-device (live WindowServer display required): `#[ignore]` by default; run
    // with `cargo test -p rustty-renderer --release -- --ignored cvdisplaylink`.
    // Bounded by a 2s guard thread so a headless run returns instead of hanging
    // (it then FAILS the `TICKS>=1` assert rather than proving a vsync — correct).
    #[test]
    #[ignore = "on-device: requires a live display (CVDisplayLink); heavy-tier KAT"]
    fn kat_vsync_displaylink_notifies_async() {
        // Reset the tick counter (a prior test in the same process may have run).
        TICKS.store(0, Ordering::SeqCst);

        // 1) xev loop + the Async that plays `thr.draw_now`.
        let mut loop_ = Loop::init(rustty_xev::Options::default()).unwrap();
        let draw_now = Async::init().unwrap();

        // Count loop wakeups; rearm so multiple vsync ticks accumulate. (Wakeup
        // count is diagnostic only; the vsync GATE is `TICKS`, asserted below.)
        let mut count: u32 = 0;
        let mut c_wait = Completion::default();
        let cb = |ud: *mut u32,
                  _l: *mut Loop,
                  _c: *mut Completion,
                  r: core::result::Result<(), WaitError>| {
            r.unwrap();
            unsafe { *ud += 1 };
            CallbackAction::Rearm
        };
        unsafe {
            draw_now.wait(
                &mut loop_ as *mut Loop,
                core::ptr::addr_of_mut!(c_wait),
                core::ptr::addr_of_mut!(count),
                cb,
            );
        }

        // 2) The CVDisplayLink bound to the active display(s); its callback bumps
        //    `TICKS` then notifies `draw_now`. `set_output_callback` boxes the handle.
        let mut dl = DisplayLink::create_with_active_cg_displays().unwrap();
        dl.set_output_callback(draw_now).unwrap();
        dl.start().unwrap();
        assert!(dl.is_running());

        // 3) Guard: after 2s notify `draw_now` ourselves so `run(Once)` can never
        //    hang on a headless box (no vsync tick). `Async` is Copy+Send. This
        //    notify BYPASSES `output_callback`, so it does NOT bump `TICKS`.
        let guard_async = draw_now;
        let done = std::sync::Arc::new(AtomicU32::new(0));
        let done_g = done.clone();
        let guard = std::thread::spawn(move || {
            for _ in 0..20 {
                if done_g.load(Ordering::SeqCst) != 0 {
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            let _ = guard_async.notify(); // unblock run(Once) on headless
        });

        // 4) Block until the FIRST wakeup (a vsync tick on-device, else the guard).
        loop_.run(RunMode::Once).unwrap();
        done.store(1, Ordering::SeqCst);
        guard.join().unwrap();

        // 5) THE W2-T3 GATE: at least one REAL CVDisplayLink tick fired through
        //    `output_callback` (bumping `TICKS`) — distinct from the guard's
        //    fallback notify, which never touches `output_callback`. On a headless
        //    box `TICKS == 0` and this assert FAILS (not hangs) — correct: the
        //    on-device `--ignored` run is the true gate. `count` (>=1 either way)
        //    is diagnostic, not the proof.
        assert!(
            TICKS.load(Ordering::SeqCst) >= 1,
            "no CVDisplayLink vsync tick observed (count={count}); on a live display \
             the output callback must fire at least once"
        );

        // 6) Teardown (mirrors Renderer::loop_exit + deinit): stop, release, deinit.
        dl.stop().unwrap();
        dl.release();
        let mut draw_now = draw_now;
        draw_now.deinit();
        loop_.deinit();
    }
}
