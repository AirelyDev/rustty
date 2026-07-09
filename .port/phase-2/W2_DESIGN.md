# W2 Design — macOS On-Screen Windowing Sub-project #1, Chunk W2 (CVDisplayLink)

**Faithful-port design for W2-T1 / W2-T2 / W2-T3** (all in `rustty-renderer`, apprt-free). Governing plan:
`docs/superpowers/plans/2026-07-08-macos-windowing-sub1.md` (Chunk W2 + the **Canonical Interfaces** and **Global Constraints** — AUTHORITATIVE). This doc resolves the FFI vocabulary, the apprt-free callback seam, and pins the exact edits with real line numbers plus the runnable vsync KAT.

**Disposition: `ready`.** No blocker. The CVDisplayLink C API is bound via `extern "C"` + `#[link(name = "CoreVideo", kind = "framework")]` (VERIFIED viable; rationale in §1). The apprt-free callback seam is `loop_enter(&mut self, draw_now: rustty_xev::Async)` — a `Copy`+`Send` handle that names no apprt/core `Thread` type (§4). W2-T1 (the primitive), the field wiring, `deinit` teardown un-defer, `has_vsync`/`set_macos_display_id`/`set_focus`/`set_visible`/`displayLinkCallback`/`loop_exit`, and the KAT are **fully runnable**; only `loop_enter`'s transitive `self.api.loop_enter()` leaf stays deferred (metal_impl `unimplemented!()`, W3) — the INHABITED-placeholder doctrine (§4.4).

**Fold-phase revision (adversary fixes applied — all resolvable, no blocker survives):**
1. **Error-conversion (was BLOCKER).** `generic::Error` gains an `impl From<cvdisplaylink::Error> for Error` (maps `InvalidOperation → OutOfMemory`, keeping the reference `createWithActiveCGDisplays → error.OutOfMemory` faithful) so the two `?` sites — `create_with_active_cg_displays()?` (init) and `set_output_callback(draw_now)?` (loop_enter) — compile. See §2.2 + §3.2/§3.4.
2. **WaitError export (was BLOCKER).** Add `pub use async_watcher::WaitError;` to `rustty-xev/src/lib.rs` (an export-gap fix in the same class as the existing `TimerRunError` re-export) so the KAT's `use rustty_xev::WaitError` resolves. See §3.0.
3. **KAT strengthened (W2-T3).** The KAT now threads a `#[cfg(test)] static TICKS: AtomicU32` bumped INSIDE `output_callback` (before `notify`) and asserts `TICKS >= 1`. The 2 s guard thread's fallback `notify()` bypasses `output_callback`, so `TICKS` counts ONLY real CVDisplayLink vsync ticks — the `--ignored` on-device run is now a true vsync proof, not satisfied by the guard. See §5.
4. **Aliasing shape (§2/§2.1/§6).** `draw_now` is now `Option<Box<Async>>` — a SEPARATE heap allocation (the faithful analogue of the reference's separate `renderer.Thread.draw_now` home), so the CoreVideo callback's `*const Async` points OUTSIDE the `Renderer` `&mut` footprint and no `&mut self` method reborrows over the aliased bytes. The soundness claim is scoped to real-execution/ASan (R6: CVDisplayLink is Miri-excluded; per `shaper-hook-sb-clean-interleave` we do NOT hand-assert full SB/TB). See §2.1/§6.
5. **init errdefer.** A concrete disarm-on-success RAII guard (`DisplayLinkErrdefer`, owns the `Option<DisplayLink>`; `Drop` calls `release()`; `disarm()` extracts on success) releases the freshly-created +1 `CVDisplayLinkRef` on any early `?`-return in `init`. See §3.2 + §6.

Reference oracle (read-only): `/Users/gael/projects/ghostty`
- `pkg/macos/video/display_link.zig` — the whole `DisplayLink` opaque wrapper (`createWithActiveCGDisplays`/`release`/`start`/`stop`/`isRunning`/`setCurrentCGDisplay`/`setOutputCallback`).
- `pkg/macos/video/c.zig` — `pub const c = @import("../main.zig").c` (the `@cImport` of `<CoreVideo/CoreVideo.h>`); this is the raw-C source of every symbol below.
- `src/renderer/generic.zig` — `DisplayLink` alias (39-40), field (197-200), `init` display-link create (692-699) + field set (786), `deinit` teardown arm (807-813), `threadEnter`/`threadExit` (881-894), `loopEnter` (897-916), `loopExit` (919-933), `displayLinkCallback` (986-994), `setMacOSDisplayID` (1001-1009), `hasVsync` (1017-1024), `setFocus` (1029-1048), `setVisible` (1053-1065).
- `src/renderer/Metal.zig` — `loopEnter` (164-170) [the `api.loopEnter` hook, distinct from the CVDisplayLink block — W3, currently `unimplemented!()` in metal_impl].
- `src/renderer/Thread.zig` — `draw_now: xev.Async` (67), the field `loopEnter` passes as `&thr.draw_now`.

Target files:
- `crates/rustty-renderer/src/metal/cvdisplaylink.rs` — **NEW** (W2-T1 + the KAT, W2-T3).
- `crates/rustty-renderer/src/generic.rs` — W2-T2 edits (real line numbers below).
- `crates/rustty-renderer/src/metal.rs` — add `pub mod cvdisplaylink;`.
- `crates/rustty-renderer/Cargo.toml` — add `rustty-xev` workspace dep.

---

## Global invariants (apply to every task)

- **R1** — every reference comment/behavior ported; the verbatim Zig body of any relocated/deferred seam stays as a doc-comment. No Zig byte-slices arise in W2 (the only scalar payloads are `u32`/`i32` C ints and the opaque `CVDisplayLinkRef`), so no `&[u8]`/`Vec<u8>` question.
- **R4** — `rustty-renderer` MUST NOT name any apprt type, and MUST NOT name the core `renderer.Thread`. The reference `loopEnter(self, thr: *renderer.Thread)` is adapted to `loop_enter(self, draw_now: rustty_xev::Async)` — the exact field `thr.draw_now`, a `Copy` handle, passed by the caller. `rustty-xev` is a leaf (deps `rustty-global` + `libc` only) → adding the renderer→xev edge introduces **no cycle** (`check_acyclic.sh`/`cargo_cycle_check.py` gate). **VERIFIED**: `rustty-xev/Cargo.toml` `[dependencies]` = `rustty-global`, `libc`.
- **R6** — every W2 test is FFI (live CoreVideo framework) ⇒ **Miri-excluded, ASan-eligible**; add the KAT name to `.port/phase-2/MIRI_EXCEPTIONS.md`. CVDisplayLink cannot run under Miri (foreign function). The callback→`Async`→loop raw-pointer aliasing shape is analyzed for SB/TB soundness in §6 (no new hand-reasoned raw-alias into a `&mut`-reborrowed field; the userdata is a `Copy`+`Send` `Async` read-only through a `*const`).
- **macOS-only** — `crate::metal` and `crate::generic` are already `#[cfg(target_os = "macos")]` (`lib.rs:78,121`). The reference `const DisplayLink = switch (builtin.os.tag) { .macos => *…, else => void }` therefore collapses to the **always-macOS** arm in this crate: every `if (comptime DisplayLink == void) return;` guard becomes an **unconditional non-return** (documented per-site). No non-macOS `void` fallback is compiled.
- **Headless-friendly** — the vsync KAT (§5) is `#[ignore]` (an on-device KAT, per the `heavy-tier-strategy` "GPU accepts no unit-gate" memory) AND is internally bounded by a guard thread so it never hangs a headless CI even if manually un-ignored.

---

## 1. FFI decision — `extern "C"` + `#[link(framework = CoreVideo)]` (NOT objc2-core-video)

**Decision: raw `extern "C"` bindings in `cvdisplaylink.rs`, framework-linked via `#[link(name = "CoreVideo", kind = "framework")]` on the `extern "C"` block.** This is locked, not a fork left open.

Rationale:
1. **Fidelity.** The reference `pkg/macos/video/c.zig` is a raw `@cImport("<CoreVideo/CoreVideo.h>")` — the Zig wrapper calls the bare C symbols (`c.CVDisplayLinkCreateWithActiveCGDisplays`, …). `extern "C"` is the 1:1 mirror; it introduces no vocabulary the reference does not already use.
2. **No new-crate / feature-gate risk.** `objc2-core-video` is **not** in the resolved dependency graph (`find ~/.cargo … objc2-core-video` → absent) and `CVDisplayLink` is an Apple-**deprecated** C API (superseded by `CADisplayLink` in macOS 14). Whether a given `objc2-core-video` release exposes `CVDisplayLink*` behind a feature is unverifiable offline and would add a network-fetched dep + feature-gate churn for **7 functions + 2 scalar typedefs**. The `extern "C"` surface is tiny, self-contained, and guaranteed to resolve against the system framework (the symbols exist regardless of deprecation — deprecation is a header annotation, not a link removal).
3. **The only two Foundation-ish types are trivial C scalars** — `CGDirectDisplayID` = `u32`, `CVReturn` = `i32` (`kCVReturnSuccess = 0`), `CVOptionFlags` = `u64`. No `objc2` `Retained`/message-send is needed (CVDisplayLink is CF-style manual retain/release via `CVDisplayLinkRetain`/`Release`, bound directly). This avoids pulling `objc2-core-graphics` just for the `u32` display-id typedef.

The framework link: the crate already links Metal/Foundation/QuartzCore via the objc2 crates; CoreVideo is added by the `#[link(name = "CoreVideo", kind = "framework")]` attribute on the `extern "C"` block in `cvdisplaylink.rs` (module is `#[cfg(target_os = "macos")]` via `metal.rs`, so the attribute never compiles off-macOS). No `build.rs` change.

### 1.1 The C symbols (exact signatures — from `<CoreVideo/CVDisplayLink.h>`)

```rust
// crates/rustty-renderer/src/metal/cvdisplaylink.rs  (extern block)
use core::ffi::c_void;

/// Opaque `CVDisplayLinkRef` (reference `pub const DisplayLink = opaque {}`).
#[repr(C)]
pub struct CVDisplayLink {
    _private: [u8; 0],
}
pub type CVDisplayLinkRef = *mut CVDisplayLink;

/// `CVReturn` (`int32_t`); success is `kCVReturnSuccess == 0`.
pub type CVReturn = i32;
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

#[link(name = "CoreVideo", kind = "framework")]
extern "C" {
    fn CVDisplayLinkCreateWithActiveCGDisplays(out: *mut CVDisplayLinkRef) -> CVReturn;
    fn CVDisplayLinkSetOutputCallback(
        dl: CVDisplayLinkRef,
        callback: CVDisplayLinkOutputCallback,
        user_info: *mut c_void,
    ) -> CVReturn;
    fn CVDisplayLinkSetCurrentCGDisplay(dl: CVDisplayLinkRef, display_id: CGDirectDisplayID) -> CVReturn;
    fn CVDisplayLinkStart(dl: CVDisplayLinkRef) -> CVReturn;
    fn CVDisplayLinkStop(dl: CVDisplayLinkRef) -> CVReturn;
    fn CVDisplayLinkIsRunning(dl: CVDisplayLinkRef) -> u8; // `Boolean`
    fn CVDisplayLinkRelease(dl: CVDisplayLinkRef);
}
```

Note: `CVDisplayLinkSetCurrentCGDisplayFromMetalDevice` (mentioned in the plan brief) is **NOT** in the reference `display_link.zig` wrapper — the reference only ever calls `setCurrentCGDisplay(display_id: CGDirectDisplayID)` (from `setMacOSDisplayID`). Out of scope; not bound. Likewise `CVDisplayLinkRetain` is unused (the create returns +1; `release` is the only refcount op the wrapper performs — see the `deinit`/`errdefer` teardown).

---

## 2. W2-T1 — `cvdisplaylink.rs`: the `DisplayLink` wrapper

Faithful port of `pkg/macos/video/display_link.zig`. The reference is an `opaque` type = a bare pointer; every method casts `@ptrCast(self)` to the `CVDisplayLinkRef`. The Rust wrapper is a thin newtype over the `NonNull<CVDisplayLink>` **plus** the apprt-free userdata home (§2.1 — the one deliberate, R4-forced divergence).

```rust
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
//! collapses it to a single concrete `set_output_callback(&self, *const Async)`
//! that installs a fixed `extern "C"` trampoline running the fixed
//! `displayLinkCallback` body (notify the `Async`). This mirrors how the rest of
//! this port collapses the comptime `GenericRenderer` hierarchy onto the Metal leaf.

use core::ffi::c_void;
use core::ptr::NonNull;
use rustty_xev::Async;

// … the `extern "C"` block + typedefs from §1.1 …

/// Reference `DisplayLink.Error = error{ InvalidOperation }`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
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
/// `&mut` footprint (§2.1/§6). See §2.1/§6.
#[derive(Debug)]
pub struct DisplayLink {
    /// The `CVDisplayLinkRef` (reference `@ptrCast(self)`). Non-null: the ctor
    /// rejects a null result exactly as the reference `orelse error.OutOfMemory`.
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
    /// reassigned while the link runs. See §2.1/§6.
    draw_now: Option<Box<Async>>,
}

// SAFETY: `link` is a CoreVideo handle used only from the render thread's
// start/stop/release + the CoreVideo-owned callback thread (which touches only
// `draw_now`, a `Send` `Async`). Mirrors the reference `*DisplayLink` crossing
// into the render thread. (Only add `unsafe impl Send`/`Sync` if a call site needs
// it — the KAT keeps the `DisplayLink` on one thread, so it is likely unneeded;
// left as a note.)

impl DisplayLink {
    /// Reference `createWithActiveCGDisplays() Allocator.Error!*DisplayLink`.
    ///     var result: ?*DisplayLink = null;
    ///     if (c.CVDisplayLinkCreateWithActiveCGDisplays(@ptrCast(&result)) != kCVReturnSuccess)
    ///         return error.OutOfMemory;
    ///     return result orelse error.OutOfMemory;
    pub fn create_with_active_cg_displays() -> Result<DisplayLink, Error> {
        let mut result: CVDisplayLinkRef = core::ptr::null_mut();
        // SAFETY: `&mut result` is a valid out-pointer; CoreVideo writes a +1 ref.
        let rc = unsafe { CVDisplayLinkCreateWithActiveCGDisplays(&mut result) };
        if rc != kCVReturnSuccess {
            return Err(Error::InvalidOperation); // reference `error.OutOfMemory`; see note
        }
        // reference `result orelse error.OutOfMemory`
        match NonNull::new(result) {
            Some(link) => Ok(DisplayLink { link, draw_now: None }),
            None => Err(Error::InvalidOperation),
        }
    }

    /// Reference `release(self) void { c.CVDisplayLinkRelease(@ptrCast(self)); }`.
    /// MANUAL (no `Drop`) — faithful to the Zig `opaque` whose release is explicit
    /// (called by `Renderer.deinit`'s teardown arm / the `init` `errdefer`). A
    /// `Drop` would double-free against those explicit `release()` calls.
    pub fn release(&self) {
        // SAFETY: `link` is a valid +1 CVDisplayLinkRef; caller guarantees the link
        // is stopped and no further method runs (Renderer::deinit / errdefer).
        unsafe { CVDisplayLinkRelease(self.link.as_ptr()) };
    }

    /// Reference `start(self) Error!void`.
    pub fn start(&self) -> Result<(), Error> {
        let rc = unsafe { CVDisplayLinkStart(self.link.as_ptr()) };
        if rc != kCVReturnSuccess { Err(Error::InvalidOperation) } else { Ok(()) }
    }

    /// Reference `stop(self) Error!void`.
    pub fn stop(&self) -> Result<(), Error> {
        let rc = unsafe { CVDisplayLinkStop(self.link.as_ptr()) };
        if rc != kCVReturnSuccess { Err(Error::InvalidOperation) } else { Ok(()) }
    }

    /// Reference `isRunning(self) bool { return c.CVDisplayLinkIsRunning(...) != 0; }`.
    pub fn is_running(&self) -> bool {
        unsafe { CVDisplayLinkIsRunning(self.link.as_ptr()) != 0 }
    }

    /// Reference `setCurrentCGDisplay(self, display_id: CGDirectDisplayID) Error!void`.
    pub fn set_current_cg_display(&self, display_id: CGDirectDisplayID) -> Result<(), Error> {
        let rc = unsafe { CVDisplayLinkSetCurrentCGDisplay(self.link.as_ptr(), display_id) };
        if rc != kCVReturnSuccess { Err(Error::InvalidOperation) } else { Ok(()) }
    }

    /// Reference `setOutputCallback(self, comptime Userdata, comptime callbackFn, userinfo)`.
    /// MONOMORPHIZED to the single instantiation `(xev.Async, &displayLinkCallback,
    /// &thr.draw_now)`. Stores `draw_now` wrapper-local (its stable address is the
    /// faithful `&thr.draw_now`) and installs the fixed `output_callback` trampoline.
    ///
    /// R1 — the reference comment is preserved verbatim:
    ///   // Note: this purposely throws away a ton of arguments I didn't need.
    ///   // It would be trivial to refactor this into Zig types and properly
    ///   // pass this through.
    pub fn set_output_callback(&mut self, draw_now: Async) -> Result<(), Error> {
        // Store the userdata FIRST (on the heap, via `Box`) so its payload address
        // is stable before CoreVideo captures it. Written exactly once; the boxed
        // `Async` is never moved, reborrowed `&mut`, or reassigned while the link
        // runs (see §6 / the field doc).
        self.draw_now = Some(Box::new(draw_now));
        // The stable `*const Async` CoreVideo hands back to `output_callback`. Taken
        // as a `*const` to the boxed pointee (`&**box` coerced to `*const`) — a
        // separate heap allocation, so NO `Renderer` `&mut` reborrow ever covers
        // these bytes, and NO intervening `&mut Async` is minted over the location
        // the callback thread reads (rust-mut-args-strongly-protected discipline; §6).
        let ud: *const Async = self.draw_now.as_deref().expect("just set");
        let rc = unsafe {
            CVDisplayLinkSetOutputCallback(
                self.link.as_ptr(),
                output_callback,
                ud as *mut c_void,
            )
        };
        if rc != kCVReturnSuccess { Err(Error::InvalidOperation) } else { Ok(()) }
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
    // only on a genuine display-link tick — the on-device gate asserts `TICKS>=1`
    // (see §5). Bumped BEFORE `notify()` so the KAT never observes a wakeup whose
    // tick has not yet been recorded.
    #[cfg(test)]
    tests::TICKS.fetch_add(1, core::sync::atomic::Ordering::SeqCst);
    // SAFETY: `ud` is the stable boxed `Async` stored by `set_output_callback`,
    // alive for the running link (separate heap allocation; the link is stopped
    // before the box drops). `Async` is `Copy`; read by value, no aliasing `&mut`.
    // `notify()` only sends to the mach port.
    let draw_now: Async = unsafe { *ud };
    if let Err(_err) = draw_now.notify() {
        // reference: log.err("error notifying draw_now err={}", .{err});
        // (rustty log seam — mirror the existing `log`/`tracing` usage in generic.rs)
    }
    kCVReturnSuccess // reference `return c.kCVReturnSuccess;`
}
```

Note on the `create` error: the reference maps CoreVideo failure to `error.OutOfMemory` (its `Allocator.Error`). Rust `create_with_active_cg_displays` returns the wrapper's own `Error::InvalidOperation` because `rustty-renderer`'s init error channel is `generic::Error`, not an allocator error; the reference `errdefer`/`try` semantics are preserved (init aborts + releases). Document this at the call site (§3, the init edit).

### 2.2 `From<cvdisplaylink::Error> for generic::Error` (the `?`-conversion glue) — REQUIRED

The two propagation sites — `DisplayLink::create_with_active_cg_displays()?` in `Renderer::init` (§3.2) and `display_link.set_output_callback(draw_now)?` in `Renderer::loop_enter` (§3.4) — use `?` INTO `generic::Error`. `generic::Error` (verified variants `MetalFailed`/`OutOfMemory`/`Defunct`/`ImageDecode`; the `From`-impl list ends at `From<shape::coretext::Error>`) has **no** conversion from the CVDisplayLink error, so as written neither `?` compiles. Add this `From` impl **in `generic.rs`, alongside the other leaf-error `From` impls** (the block that already carries `From<crate::metal::metal_impl::Error>` / `From<crate::metal::frame::Error>` / `From<shape::coretext::Error>`):

```rust
    // `Renderer::init`'s `DisplayLink::create_with_active_cg_displays()?` and
    // `Renderer::loop_enter`'s `display_link.set_output_callback(draw_now)?`
    // propagate the CVDisplayLink wrapper error (W2). The reference
    // `createWithActiveCGDisplays` maps CoreVideo failure to `error.OutOfMemory`
    // (its `Allocator.Error`); we mirror that by folding the sole
    // `cvdisplaylink::Error::InvalidOperation` into the aggregate `OutOfMemory`
    // member — keeping the reference `try … catch error.OutOfMemory` faithful.
    impl From<crate::metal::cvdisplaylink::Error> for Error {
        fn from(_: crate::metal::cvdisplaylink::Error) -> Self {
            Error::OutOfMemory
        }
    }
```

(`cvdisplaylink::Error` has the single variant `InvalidOperation`, so the `_` catch-all is exhaustive and faithful. This is the ONLY thing that makes the two `?` sites compile — without it, W2-T2 does not build.)

### 2.1 Why `draw_now` lives in the wrapper (the one R4-forced divergence)

The CoreVideo output callback needs a **stable `void*` userInfo** it dereferences on the display's own thread. In the reference this is `&thr.draw_now` — a pointer into the long-lived `renderer.Thread`. R4 forbids `rustty-renderer` naming `renderer.Thread`, so the port cannot hold a `*Thread`. Options considered:

1. **Store the `Async` in the `Renderer` struct** (a new `Renderer.draw_now` field). Rejected: it changes the `Renderer` struct's field list vs the reference (which has no such field) and spreads the seam across the struct.
2. **Store the `Copy` `Async` inline in the wrapper** (`draw_now: Option<Async>`). Rejected on the fold-phase re-review: the wrapper lives INSIDE `Renderer.display_link`, so the boxed-in-place `Async` sits within the `Renderer` `&mut` footprint — a `&mut self` method (`loop_enter`/`set_focus`/`deinit`) reborrows over the very bytes the CoreVideo callback aliases on the display thread. The reference AVOIDS this by keeping `draw_now` in a SEPARATE `renderer.Thread` (no renderer `&mut` ever covers it); an inline field does not reproduce that separation.
3. **CHOSEN — store the `Async` in a `Box` in the wrapper** (`draw_now: Option<Box<Async>>`). The wrapper is exactly the object whose lifetime matches the callback registration (created in `init`, released in `deinit`/`errdefer`, stopped in `loop_exit`), and it already lives inside `Renderer.display_link` — a field that IS in the reference. The `Box` puts the `Async` pointee in its OWN heap allocation, so its address is stable across any wrapper/`Renderer` move AND lies OUTSIDE the `Renderer` `&mut` footprint — the faithful analogue of the reference's separate `renderer.Thread.draw_now` home (which the renderer likewise never covers with a `&mut`). The seam stays confined to one wrapper field, the `Renderer` struct stays field-faithful, and the stable-address contract matches the reference's (`&thr.draw_now` ↔ the boxed `&**self.display_link.draw_now`). Cost: one small heap allocation per link — accepted, because it is what makes the callback-aliasing shape sound (§6) rather than merely "probably fine because nothing moves".

---

## 3. W2-T2 — `generic.rs` edits (real line numbers, against the file as it stands)

### 3.0 Imports + Cargo + module (prep)

- **`crates/rustty-renderer/Cargo.toml`** `[dependencies]` (after `rustty-render-types = { workspace = true }`, ~line where the workspace deps list): add
  ```toml
  # rustty-xev: the `xev::Async` (Copy+Send mach-port wakeup) the CVDisplayLink
  # output callback notifies — the reference `thr.draw_now: xev.Async` (R4: this is
  # NOT an apprt type; xev is a leaf crate, no dependency cycle).
  rustty-xev = { workspace = true }
  ```
  Confirm `rustty-xev` has a `[workspace.dependencies]` entry in the root `Cargo.toml`; add one if absent (it is already a workspace member).
- **`crates/rustty-renderer/src/metal.rs:45`** — add after `pub mod texture;`:
  ```rust
  pub mod cvdisplaylink; // W2 — the CoreVideo CVDisplayLink loop driver
  ```
- **`crates/rustty-renderer/src/generic.rs`** module imports — add `use crate::metal::cvdisplaylink::DisplayLink;` and `use rustty_xev::Async;` alongside the existing `crate::metal::*` imports.
- **`crates/rustty-xev/src/lib.rs`** — re-export `WaitError` (an export gap the KAT surfaces). Verified: `async.rs` has `pub type WaitError = MachPortError;` but its module `async_watcher` is PRIVATE (`#[path = "async.rs"] mod async_watcher;`), and lib.rs only `pub use async_watcher::Async;` — so `rustty_xev::WaitError` does not resolve. `MachPortError` IS already re-exported (via the `kqueue` block), so `WaitError` is merely the un-exported alias of an exported type. Add, next to the existing `pub use timer::RunError as TimerRunError;` (same export-gap precedent):
  ```rust
  // PORT NOTE: `Async.wait`'s callback error type (`pub const WaitError =
  // xev.Sys.MachPortError;`) must be nameable by consumers — the W2 CVDisplayLink
  // KAT (and any typed `Async.wait` callback) spells its `r: Result<(), WaitError>`
  // parameter, and the module is private. Re-export the alias (same gap the
  // `TimerRunError` re-export closed).
  #[cfg(target_os = "macos")]
  pub use async_watcher::WaitError;
  ```
  (The KAT may equivalently `use rustty_xev::MachPortError as WaitError`, but re-exporting the canonical `WaitError` name is the faithful fix and matches the reference alias `pub const WaitError = …`.)

### 3.1 Replace the placeholder `DisplayLink` struct (generic.rs **1065-1073**)

Currently:
```rust
    /// PORT NOTE (DEFERRED SEAM — macOS display link): reference … inhabited placeholder …
    #[derive(Debug)]
    pub struct DisplayLink {
        _seam: (),
    }
```
**Replace** with a re-export/`use` so the field type resolves to the real wrapper, and update the PORT NOTE to record that W2 un-defers it:
```rust
    // PORT NOTE (W2 — un-deferred): reference `const DisplayLink =
    // switch (builtin.os.tag) { .macos => *macos.video.DisplayLink, else => void }`.
    // This crate's `generic`/`metal` modules are macOS-only (`lib.rs:78,121`), so the
    // `.macos` arm is the only one compiled and `DisplayLink` is ALWAYS the real
    // CoreVideo wrapper — every `if (comptime DisplayLink == void) return;` guard in
    // the methods below collapses to an unconditional non-return. The wrapper lives
    // in `crate::metal::cvdisplaylink` (W2-T1); re-exported here so the field type
    // `display_link: Option<DisplayLink>` (below) is unchanged.
    pub use crate::metal::cvdisplaylink::DisplayLink;
```
(The `display_link: Option<DisplayLink>` field at **1250-1253** needs **no change** — the type name is identical.)

### 3.2 `Renderer::init` — create the link when `config.vsync` (generic.rs **1863-1872**)

Currently (1863-1872):
```rust
            // const display_link: ?DisplayLink = switch (builtin.os.tag) { … };
            // errdefer if (display_link) |v| v.release();
            //
            // W1: the `.macos` vsync display link is a W2 concern -> None (DEFERRALS.md #2).
            let display_link: Option<DisplayLink> = None;
```
**Replace** the `let` with the faithful create (the `.macos` arm; `DerivedConfig.vsync` exists at generic.rs:329, set from `config.window_vsync()` at 517):
```rust
            // generic.zig:692-699:
            //   const display_link: ?DisplayLink = switch (builtin.os.tag) {
            //       .macos => if (options.config.vsync)
            //           try macos.video.DisplayLink.createWithActiveCGDisplays()
            //       else null,
            //       else => null,
            //   };
            //   errdefer if (display_link) |v| v.release();
            // macOS-only crate ⇒ the `.macos` arm only. The `try` maps to `?`; the
            // reference `error.OutOfMemory` becomes `generic::Error` via the §2.2
            // `From<cvdisplaylink::Error>` impl (`InvalidOperation -> OutOfMemory`).
            //
            // errdefer: the freshly-created +1 `CVDisplayLinkRef` must be released on
            // ANY later `?`-return in `init` (reference `errdefer if (display_link)
            // |v| v.release();`). Rust has no `errdefer`; use a disarm-on-success RAII
            // guard that OWNS the `Option<DisplayLink>`. `Drop` releases; `disarm()`
            // extracts the value on success (leaving `None`, so the ensuing `Drop` is
            // a no-op) to move into the `Renderer`. Defined as a `fn`-local type at
            // the top of `init` (or a module-private helper):
            //
            //   struct DisplayLinkErrdefer {
            //       dl: Option<DisplayLink>,
            //   }
            //   impl DisplayLinkErrdefer {
            //       /// Disarm on success: take the link out (leaving `None`, so the
            //       /// subsequent `Drop` releases nothing) to move into the Renderer.
            //       fn disarm(mut self) -> Option<DisplayLink> {
            //           self.dl.take()
            //       }
            //   }
            //   impl Drop for DisplayLinkErrdefer {
            //       fn drop(&mut self) {
            //           // reference `errdefer if (display_link) |v| v.release();`
            //           if let Some(dl) = self.dl.as_ref() {
            //               dl.release();
            //           }
            //       }
            //   }
            //
            // The `?` inside the `Some(...)` initializer returns BEFORE the guard is
            // built (nothing created ⇒ nothing to leak); every `?` AFTER this drops
            // the guard ⇒ `release()`.
            let display_link_errdefer = DisplayLinkErrdefer {
                dl: if self_config_vsync {
                    Some(DisplayLink::create_with_active_cg_displays()?)
                } else {
                    None
                },
            };
```
where `self_config_vsync` reads the derived config vsync (`config.vsync` on the local `DerivedConfig` built earlier in `init` — use the actual local binding name; grep the init body for the `DerivedConfig`/`config` local).

**errdefer fidelity + the struct build (~1961).** The guard OWNS `display_link` across every intermediate `?` in `init`. Immediately before the successful `Renderer { … }` literal at **1961**, disarm and extract:
```rust
            // Disarm the errdefer just before the successful build — extract the
            // link to move into the Renderer (any earlier `?`-return already ran
            // the guard's `Drop` → `release()`).
            let display_link: Option<DisplayLink> = display_link_errdefer.disarm();
            // … then the existing struct literal, unchanged: `Renderer { …, display_link, … }`
```
This is a concrete disarm-on-success RAII guard (not a `&`-borrow guard, which would conflict with moving `display_link` into the struct): owning the value lets `disarm()` hand it off by move while `Drop` covers every error path. It matches the crate's W1 errdefer idiom.

### 3.3 `Renderer::deinit` — UN-DEFER the teardown arm (generic.rs **2052-2061**)

Currently (2052-2061):
```rust
            // if (DisplayLink != void) {
            //     if (self.display_link) |display_link| {
            //         display_link.stop() catch {};
            //         display_link.release();
            // …
            // (807-812) — W1: `display_link` is always `None` (the `.macos` vsync
            // DisplayLink lands in W2, DEFERRALS.md #2); teardown (stop/release via
            // … deferred.
```
**Replace** the deferred note with the real teardown (generic.zig:807-813):
```rust
            // generic.zig:807-813:
            //   if (DisplayLink != void) {
            //       if (self.display_link) |display_link| {
            //           display_link.stop() catch {};
            //           display_link.release();
            //       }
            //   }
            // macOS-only crate ⇒ `DisplayLink != void` is always true (no comptime
            // guard). `stop() catch {}` = ignore the stop error (link may never have
            // started / its view is gone); then `release()` the +1 ref from `init`.
            if let Some(display_link) = self.display_link.as_ref() {
                let _ = display_link.stop(); // catch {}
                display_link.release();
            }
```
(Place this at the same position the reference has it — after `self.swap_chain.deinit();` at 2050, before `self.cells.deinit();` at 2064 — which is exactly where the current commented arm sits.)

### 3.4 NEW methods — `threadEnter`/`threadExit`/`loopEnter`/`loopExit`/`displayLinkCallback`/`setMacOSDisplayID`/`hasVsync`/`setFocus`/`setVisible`

None of these exist yet in `generic.rs` (grep confirmed only `finalize_surface_init`/`mark_dirty` are present). Add them **adjacent to the existing lifecycle hooks** — insert `thread_enter`/`thread_exit` right after `finalize_surface_init` (**2123**), and the rest in reference order around `mark_dirty` (**2130**). Each is a faithful body; the `@hasDecl(GraphicsAPI, "X")` probes resolve against the concrete `Metal` (grep of `Metal.zig` shows Metal declares **only** `loopEnter` — so `threadEnter`/`threadExit`/`loopExit` hooks are no-ops, and `loopEnter` calls `self.api.loop_enter()`).

```rust
        // generic.zig:881-886 threadEnter. `@hasDecl(Metal,"threadEnter")` is FALSE
        // (Metal declares only loopEnter) ⇒ no api call; `surface` is the apprt
        // surface the hook is generic over (never named, R4).
        //   pub fn threadEnter(self, surface: *apprt.Surface) !void {
        //       if (@hasDecl(GraphicsAPI, "threadEnter")) try self.api.threadEnter(surface);
        //   }
        #[allow(clippy::not_unsafe_ptr_arg_deref)]
        pub fn thread_enter<S>(&self, rt_surface: *mut S) -> Result<(), Error> {
            let _ = rt_surface;
            Ok(())
        }

        // generic.zig:889-894 threadExit. `@hasDecl(Metal,"threadExit")` FALSE ⇒ no-op.
        pub fn thread_exit(&self) {}

        // generic.zig:897-916 loopEnter. Adapted for R4: the reference takes
        // `thr: *renderer.Thread` and reads `&thr.draw_now`; we cannot name
        // `renderer.Thread` (R4), so the caller passes the `draw_now: xev::Async`
        // field directly (Copy+Send handle — NOT an apprt type). See §4.
        //   pub fn loopEnter(self, thr: *renderer.Thread) !void {
        //       if (@hasDecl(GraphicsAPI, "loopEnter")) self.api.loopEnter();
        //       if (comptime DisplayLink == void) return;      // (macOS: never)
        //       const display_link = self.display_link orelse return;
        //       try display_link.setOutputCallback(xev.Async, &displayLinkCallback, &thr.draw_now);
        //       display_link.start() catch {};
        //   }
        pub fn loop_enter(&mut self, draw_now: Async) -> Result<(), Error> {
            // Metal DECLARES loopEnter ⇒ this hook fires. It installs the
            // IOSurfaceLayer display callback and needs a live NSView — DEFERRED to
            // W3 (metal_impl `loop_enter` is `unimplemented!("windowing-host")`).
            // INHABITED-placeholder doctrine: the method is faithfully filled; its
            // transitive leaf `self.api.loop_enter()` stays the deferred W3 seam.
            self.api.loop_enter();
            // `if (comptime DisplayLink == void) return;` — macOS: DisplayLink is
            // never void; no early return.
            let Some(display_link) = self.display_link.as_mut() else {
                return Ok(());
            };
            display_link.set_output_callback(draw_now)?;
            let _ = display_link.start(); // catch {}
            Ok(())
        }

        // generic.zig:919-933 loopExit. `@hasDecl(Metal,"loopExit")` FALSE ⇒ no api
        // call ⇒ FULLY runnable (unlike loopEnter). Stop the link; error ignored.
        //   pub fn loopExit(self) void {
        //       if (@hasDecl(GraphicsAPI, "loopExit")) self.api.loopExit();
        //       if (comptime DisplayLink == void) return;
        //       const display_link = self.display_link orelse return;
        //       display_link.stop() catch {};
        //   }
        pub fn loop_exit(&self) {
            if let Some(display_link) = self.display_link.as_ref() {
                let _ = display_link.stop(); // catch {}
            }
        }

        // generic.zig:986-994 displayLinkCallback. The body is INLINED into the
        // `cvdisplaylink::output_callback` trampoline (§2) because a CoreVideo
        // `extern "C"` callback cannot be a `Self` method and must run without a
        // `Renderer` receiver. Kept as an R1 doc-anchor here; the live copy is in
        // cvdisplaylink.rs.
        //   fn displayLinkCallback(_: *macos.video.DisplayLink, ud: ?*xev.Async) void {
        //       const draw_now = ud orelse return;
        //       draw_now.notify() catch |err| log.err("error notifying draw_now err={}", .{err});
        //   }

        // generic.zig:1001-1009 setMacOSDisplayID. FULLY runnable.
        //   pub fn setMacOSDisplayID(self, id: u32) !void {
        //       if (comptime DisplayLink == void) return;
        //       const display_link = self.display_link orelse return;
        //       log.info("updating display link display id={}", .{id});
        //       display_link.setCurrentCGDisplay(id) catch |err|
        //           log.warn("error setting display link display id err={}", .{err});
        //   }
        pub fn set_macos_display_id(&self, id: u32) -> Result<(), Error> {
            let Some(display_link) = self.display_link.as_ref() else {
                return Ok(());
            };
            // log.info("updating display link display id={}", id);
            if let Err(_err) = display_link.set_current_cg_display(id) {
                // log.warn("error setting display link display id err={}", err);
            }
            Ok(())
        }

        // generic.zig:1017-1024 hasVsync. FULLY runnable.
        //   pub fn hasVsync(self) bool {
        //       if (comptime DisplayLink == void) return false;
        //       const display_link = self.display_link orelse return false;
        //       return display_link.isRunning();
        //   }
        pub fn has_vsync(&self) -> bool {
            match self.display_link.as_ref() {
                Some(display_link) => display_link.is_running(),
                None => false,
            }
        }

        // generic.zig:1029-1048 setFocus. FULLY runnable. NB: keeps the reference
        // `assert(self.focused != focus)` + the custom-shader flag + start/stop.
        //   pub fn setFocus(self, focus: bool) !void {
        //       assert(self.focused != focus);
        //       self.focused = focus;
        //       self.custom_shader_focused_changed = true;
        //       if (comptime DisplayLink != void) link: {
        //           const display_link = self.display_link orelse break :link;
        //           if (focus) display_link.start() catch {} else display_link.stop() catch {};
        //       }
        //   }
        pub fn set_focus(&mut self, focus: bool) -> Result<(), Error> {
            assert!(self.focused != focus);
            self.focused = focus;
            self.custom_shader_focused_changed = true;
            if let Some(display_link) = self.display_link.as_ref() {
                let _ = if focus { display_link.start() } else { display_link.stop() };
            }
            Ok(())
        }

        // generic.zig:1053-1065 setVisible. FULLY runnable.
        //   pub fn setVisible(self, visible: bool) void {
        //       if (comptime DisplayLink != void) link: {
        //           const display_link = self.display_link orelse break :link;
        //           if (visible and self.focused) display_link.start() catch {}
        //           else display_link.stop() catch {};
        //       }
        //   }
        pub fn set_visible(&self, visible: bool) {
            if let Some(display_link) = self.display_link.as_ref() {
                let _ = if visible && self.focused {
                    display_link.start()
                } else {
                    display_link.stop()
                };
            }
        }
```

**Field prerequisites** — verify these `Renderer` fields exist (referenced by `set_focus`/`set_visible`); grep for them in generic.rs and, if any is still a deferred placeholder, they are set here per the reference:
- `focused: bool` (reference `self.focused`).
- `custom_shader_focused_changed: bool` (reference `self.custom_shader_focused_changed`).
Both already exist in the ported `Renderer` (they are read by the custom-shader draw path); confirm and wire. If `focused` is absent, add it (reference field, default per `init`).

### 3.5 Test-builder / offscreen paths (generic.rs **5937**, config **5809**)

- The offscreen test builder at **5937** (`display_link: None`) and its `vsync: false` config (**5809**) **stay `None`** — offscreen tests have no live display; a real `create_with_active_cg_displays()` there would bind a link with no purpose and slow the suite. No change.
- The windowed-lifecycle round-trip test (**6085** `windowed_renderer_init_then_deinit_round_trips`) exercises `init`→`deinit`; with its config `vsync: false`, `display_link` is `None` and the new `deinit` teardown arm is a no-op — still green. If you want the deinit teardown COVERED by that test, add a sibling that sets `vsync: true` and asserts `init` then `deinit` releases without leak (ASan) — but that binds a real link ⇒ `#[ignore]`/on-device (same class as the KAT).

---

## 4. The apprt-free callback seam (R4) — resolved, NOT deferred

The plan's KEY seam is satisfiable in W2 without deferring `loop_enter`'s CVDisplayLink half:

- **Reference:** `loopEnter(self, thr: *renderer.Thread)` → `setOutputCallback(xev.Async, &displayLinkCallback, &thr.draw_now)`. The callback `notify()`s `thr.draw_now`.
- **Port:** `loop_enter(&mut self, draw_now: rustty_xev::Async)`. The caller (the core render thread, a later chunk) owns the `xev::Async` and passes it by value. `Async` is `#[derive(Clone, Copy)]` + auto-`Send` (bare `port: u32`) — a **plain leaf-crate type, not an apprt/core type** — so naming it in `rustty-renderer` violates neither R4 nor the acyclic gate (`rustty-xev` deps `rustty-global`+`libc`; renderer→xev adds no cycle). `set_output_callback` stores the `Copy` handle wrapper-local and the CoreVideo callback `notify()`s it. **No `renderer.Thread`, no apprt type, is named.**

### 4.1 Divergence ledger (loop_enter signature)

`loop_enter(&mut self, draw_now: Async)` vs reference `loopEnter(self, thr: *renderer.Thread)`. This is the minimal faithful adaptation forced by R4: the reference passes `*Thread` **solely** to reach `&thr.draw_now`; the port passes that exact field. Record in `DEFERRALS.md`/the divergence log: "W2: `loop_enter` takes `draw_now: xev::Async` (the `Thread.draw_now` field) instead of `*Thread` — R4 forbids naming `renderer.Thread`; behavior identical."

### 4.2 The `loop_enter` deferred leaf (metal_impl)

`self.api.loop_enter()` (metal_impl.rs:476-479) is `unimplemented!("windowing-host: Metal::loop_enter installs the IOSurfaceLayer display callback")` — it needs a live NSView layer (W3). So `Renderer::loop_enter` **compiles and is faithful** but **panics if called** until W3 fills the Metal hook. This is the INHABITED-placeholder doctrine (`placeholder-leaf-doctrine` memory): the method is real, its deferred leaf is `unimplemented!()`. The **KAT does not call `Renderer::loop_enter`** — it drives the `DisplayLink` primitive + the callback→`Async` seam directly (§5), so the deferred Metal leaf never gates the vsync proof. `loop_exit`, in contrast, has NO Metal hook (`@hasDecl(Metal,"loopExit")` is false) and is fully runnable.

---

## 5. W2-T3 — the vsync KAT (runnable)

Lives in `cvdisplaylink.rs` `#[cfg(test)] mod tests`. Proves the full round-trip: a real `CVDisplayLink` bound to the active display(s), whose `output_callback` `notify()`s a real `xev::Async`, wakes a bounded `xev::Loop` drain. **On-device** (needs a WindowServer display) ⇒ `#[ignore]` per the heavy-tier "GPU accepts no unit-gate" memory, AND internally bounded by a guard thread so it never hangs if un-ignored on headless CI. Add the name to `.port/phase-2/MIRI_EXCEPTIONS.md` (FFI, R6).

The KAT bumps a `#[cfg(test)] static TICKS` counter **inside `output_callback`** (§2) and asserts on THAT — not on the loop-wakeup count — so the guard thread's fallback `notify()` (which bypasses `output_callback`) cannot satisfy the assertion. `TICKS` rises only on a genuine CVDisplayLink tick, so the `--ignored` on-device run is a true vsync proof (W2-T3). Keeping the counter behind `#[cfg(test)]` leaves the production trampoline branch-free.

```rust
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
        let mut loop_ = Loop::init().unwrap();
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
                if done_g.load(Ordering::SeqCst) != 0 { return; }
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
```

---

## 6. Soundness (SB/TB), Drop, errdefer

- **Callback raw-pointer aliasing.** `set_output_callback` stores `draw_now: Some(Box::new(async))` then hands CoreVideo a `*const Async` to the **boxed pointee** (via `self.draw_now.as_deref()` → `*const`, no `&mut`). The CoreVideo thread reads it **by value** (`*ud`, `Async: Copy`) and calls `notify(&self)` (reads `port`, sends a mach msg). Two properties make this sound in real execution:
  1. **Separate allocation.** The `Box` pointee is in its OWN heap allocation, distinct from the `Renderer`. A `&mut self` reborrow (`loop_enter`/`set_focus`/`deinit`) retags the `Renderer`'s storage — which contains only the `Box`'s owning pointer, NOT the pointee — so it does NOT cover the bytes the callback aliases. This is the faithful analogue of the reference keeping `draw_now` in a separate `renderer.Thread` that the renderer never reborrows. (An inline `Option<Async>` field, by contrast, WOULD sit under that `&mut` — the reason §2.1 rejected it in the fold-phase re-review.)
  2. **Read-only, never reborrowed `&mut`.** The pointee is written exactly once (in `set_output_callback`) and thereafter only READ (by value, `Copy`) on the callback thread. The render thread never forms a `&mut` to the pointee: `start`/`stop`/`is_running`/`set_current_cg_display` don't touch `draw_now`; `release`/drop happen only after `stop`. No `&**box` (which would retag the pointee `Unique`) is taken after registration.

  **Soundness scope.** This is **FFI** (live CoreVideo) ⇒ **ASan-verified, Miri-excluded** (R6: CVDisplayLink is a foreign function that cannot run under Miri). Per `shaper-hook-sb-clean-interleave` ("don't hand-reason SB; PROVE on an isolated Miri harness"), we do **NOT** assert full Stacked/Tree-Borrows correctness by hand here — there is no non-FFI Miri harness for a CoreVideo callback to prove it on. The claim is scoped to **real execution under ASan**: separate-allocation pointee + immutable-after-registration + `Copy`-by-value read ⇒ no data race, no use-after-free, no aliasing write. The `Box`-separate-allocation shape is specifically what removes the "a `&mut self` reborrow covers the exact aliased bytes" hazard the inline field had. Document the SAFETY invariants at both `set_output_callback` and `output_callback` (as sketched).
- **No `Drop` on `DisplayLink`.** Release of the `CVDisplayLinkRef` is MANUAL (`Renderer::deinit` teardown arm + the `init` errdefer), faithful to the Zig `opaque` (explicit `release`). A `Drop` releasing would double-free against those explicit calls. Trade-off: a `Renderer` dropped WITHOUT `deinit` leaks the `CVDisplayLinkRef` — identical to the reference's manual-release contract; acceptable and documented. (The `Box<Async>` payload, in contrast, IS owned by the `DisplayLink` and freed by the normal Rust drop of the field — the `Box` frees its heap `Async`; only the CoreVideo handle is manual. The link is always `stop()`-ped before the wrapper drops, so the callback can no longer read the freed box.)
- **`init` errdefer — concrete (§3.2).** `create_with_active_cg_displays()` returns a +1 ref; if a later `?` in `init` fails, that ref must be released (reference `errdefer if (display_link) |v| v.release()`). Implemented by the `DisplayLinkErrdefer` guard in §3.2: it OWNS the `Option<DisplayLink>` from creation; its `Drop` calls `release()` on any early `?`-return; `disarm()` extracts the value just before the successful `Renderer { … }` build at ~1961 (leaving `None`, so its own `Drop` then releases nothing). An owning guard (not a `&`-borrow guard) is required — a borrow would conflict with moving `display_link` into the struct literal.
- **`stop`/`start` errors ignored** (`catch {}`) at every reference site — preserved via `let _ = …`.

---

## 7. Gates & fidelity checklist

- **build / clippy -D / fmt** — `rustty-xev` dep added; `#[link(framework=CoreVideo)]` links on macOS. **Two compile-blockers closed (fold-phase):** (1) `impl From<crate::metal::cvdisplaylink::Error> for generic::Error` added in `generic.rs` (§2.2) — without it the `create_with_active_cg_displays()?`/`set_output_callback(draw_now)?` `?`-sites don't build; (2) `pub use async_watcher::WaitError;` added to `rustty-xev/src/lib.rs` (§3.0) — without it the KAT's `use rustty_xev::WaitError` doesn't resolve. Clippy: the `extern "C"` fn and `NonNull` are fine; `#[allow(non_upper_case_globals)]` on `kCVReturnSuccess` if clippy flags the C-name constant (or rename to `K_CV_RETURN_SUCCESS` with an R1 note — prefer the C name + allow for fidelity). The KAT's `#[cfg(test)] static TICKS` is `pub(super)` (named by `output_callback` as `tests::TICKS`); the bump site is `#[cfg(test)]`-gated so the production trampoline is branch-free.
- **check_acyclic.sh / cargo_cycle_check.py** — renderer→xev is acyclic (xev is a leaf). VERIFIED.
- **check_comment_coverage.py / check_comments_fidelity.py** — every reference method carries its verbatim Zig body as a doc-comment (§2, §3.4); `displayLinkCallback`'s body is anchored both at the generic.rs doc-stub (§3.4) and inlined in `output_callback`.
- **check_test_counts.py / check_tests_ported.py** — the reference `display_link.zig` has **no** `test{}` blocks; the KAT is a rustty-added KAT (like KAT 1 in async.rs), so it adds test count without a reference counterpart — record it in the KAT ledger, not the ported-test map.
- **R6 / MIRI_EXCEPTIONS.md** — add `kat_vsync_displaylink_notifies_async` (FFI/on-device, Miri-excluded, ASan-eligible, `#[ignore]`).
- **Divergence ledger** — record: (a) `loop_enter(draw_now: Async)` vs `loopEnter(thr: *Thread)` [R4]; (b) `DisplayLink.draw_now: Option<Box<Async>>` wrapper field vs reference `opaque` [R4, §2.1] — the `Box` is a deliberate heap-separate home replacing the reference's separate `renderer.Thread.draw_now` (soundness, §6), not merely a field add; (c) `create` error `generic::Error` (via `From<cvdisplaylink::Error>`, `InvalidOperation→OutOfMemory`) vs `Allocator.Error` [error-channel]; (d) `pub use async_watcher::WaitError` added to `rustty-xev` — an export-gap fix (same class as the existing `TimerRunError` re-export), not a behavioral divergence.

---

## 8. Task-by-task summary

| Task | File | Runnable now? | Notes |
|------|------|---------------|-------|
| **W2-T1** CVDisplayLink primitive | `metal/cvdisplaylink.rs` (NEW) | **Yes** | `extern "C"`+`#[link(CoreVideo)]`; wrapper with the `Box<Async>` (heap-separate) `draw_now` userdata home (§2.1/§6) |
| **W2-T2** generic.rs CVDisplayLink block | `generic.rs` (edits at 1065-1073, 1863-1872, 2052-2061, +new methods @2123/2130, +`From<cvdisplaylink::Error>` in the leaf-error `From` block §2.2) | **Mostly** | `deinit`/`loop_exit`/`has_vsync`/`set_macos_display_id`/`set_focus`/`set_visible`/`displayLinkCallback` fully runnable; `loop_enter`'s `self.api.loop_enter()` leaf is the W3 deferred `unimplemented!()` (INHABITED placeholder). Requires the §2.2 `From` impl + the §3.2 `DisplayLinkErrdefer` guard to build/leak-free |
| **W2-T3** vsync KAT | `metal/cvdisplaylink.rs` tests | **Yes** (on-device, `#[ignore]`) | Real CVDisplayLink→`xev::Async`→loop round-trip; guard-thread bounded; asserts `TICKS>=1` (real ticks counted IN `output_callback`, guard notify bypasses it) — a true vsync proof, not satisfied by the guard. Needs the §3.0 `WaitError` re-export |
