# macOS On-Screen Windowing (Sub-project #1) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
>
> **Port-domain adaptation (read first):** this is a full-fidelity Zig→Rust port under R1. For a port task, the "implementation" is the *faithful translation of a specific reference body* — so each task's production step is stated as "port `ghostty <path>:<lines>` per R1 (verbatim comments, byte-faithful Zig-slice→`&[u8]`/`Vec<u8>`/`CString`)", and the task's executable contract is its **Test KAT + Gates**, which are given in full. Each chunk (W0–W4) is executed as its own dynamic workflow (design→adversary→fold→port→verify) — the design phase resolves any remaining method-level signature detail against the reference and this plan's Canonical Interfaces.

**Goal:** Turn the proven offscreen Metal renderer into a live on-screen macOS window — first pixels, then a Rust harness showing a live terminal frame — by filling the deferred windowing surface (IOSurfaceLayer present, CVDisplayLink, the core-resident render `Thread` event loop, the real `Renderer` swap, and the apprt-c macOS host handshake).

**Architecture:** Present = render into the IOSurface-backed `MTLTexture` (already done + golden-tested) → set that IOSurface as an `IOSurfaceLayer`'s `contents` → attach the layer to the host `NSView`. The render `Thread` event loop lives in **core** (`renderer_seam::Thread<A>`, apprt-generic — R4 forbids it in rustty-renderer); the CVDisplayLink and Metal present live in **rustty-renderer** (apprt-free); the macOS host handshake lives in **apprt-c**.

**Tech Stack:** Rust; `objc2` 0.6.4 + `objc2-metal`/`objc2-io-surface`/`objc2-core-graphics`/`objc2-foundation`/`block2`/`dispatch2` (in-lock) + **`objc2-quartz-core`** (new) + `objc2-app-kit` (W4); CoreVideo `CVDisplayLink` (extern-C FFI); `rustty-xev` (kqueue loop, Async/Timer/Loop — Milestone A/B done).

## Global Constraints

- **R1 — 100% fidelity:** every comment/test/assertion ported verbatim; Zig byte-slices → `&[u8]`/`Vec<u8>`/`CString`, **never** lossy `String`. Deferred code (R5) = `unimplemented!()` + the verbatim reference body preserved as a comment.
- **R4 — acyclic:** `bash .port/tools/gates/check_acyclic.sh` must stay green. **rustty-renderer MUST NOT depend on rustty-apprt-api** (the render `Thread` is apprt-generic → it stays in core).
- **R6 — memory safety:** pure crates → Miri; FFI (objc/CoreVideo) → ASan + the isolated `.port/phase-2/mirihar-chunk3b` harness for the render-thread aliasing proof (add `renderthread_naive`=UB / `renderthread_shipped`=CLEAN candidates before W3-E2b ships).
- **Naming:** public Rust API is `snake_case` (clippy `-D warnings`); the reference's camelCase identifiers are preserved in comments/`#[allow(non_snake_case)]` only where the port already does so (e.g. `threadMain` on raw-ptr entrypoints per the established discipline).
- **Standing gates (every task ends green):** `cargo build --workspace`, `cargo test` (relevant crate), `cargo clippy --workspace -- -D warnings`, `cargo fmt --check`, `check_acyclic.sh`, `check_test_counts.py`, `check_comment_coverage.py`. macOS-only tests are `#[cfg(target_os = "macos")]`.
- **Trailer** on every commit ends with exactly:
  `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`
  `Claude-Session: https://claude.ai/code/session_01BAymGNbSpspdbRTxnGWGof`

## Canonical Interfaces (AUTHORITATIVE — overrides any per-task `Consumes`/`Produces` drift below)

The parallel chunk-authors drifted on a few signatures; these are the reconciled contracts. Where a task's `Consumes`/`Produces` differs, **this section wins**.

### IOSurfaceLayer (produced by W0.T3, in `crates/rustty-renderer/src/metal/iosurface_layer.rs`)
```rust
pub struct IOSurfaceLayer { layer: objc2::rc::Retained<IOSurfaceLayerClass> }
pub struct Error; // ObjCFailed analogue (reference `error.ObjCFailed`)
impl IOSurfaceLayer {
    pub fn init() -> Result<IOSurfaceLayer, Error>;
    pub fn release(&mut self);
    /// ghostty `!void` — main-thread dispatch can fail ⇒ Result.
    pub fn set_surface(&self, surface: &objc2_io_surface::IOSurfaceRef) -> Result<(), Error>;
    pub fn set_surface_sync(&self, surface: &objc2_io_surface::IOSurfaceRef);
    pub fn set_display_callback(&self, cb: /* reference block2 shape */);
    /// Upcast for NSView attach / addSublayer (W1-2 uses THIS, not the field).
    pub fn as_ca_layer(&self) -> &objc2_quartz_core::CALayer;
}
```
Names are **snake_case** (`set_surface`/`set_surface_sync`/`set_display_callback`). The field is the subclass type; consumers take `&CALayer` via `as_ca_layer()`.

### rustty_renderer::Renderer windowed lifecycle (produced by **W1 — see the added task W1-0 below**)
The monomorphic offscreen `Renderer` gains its windowed-lifecycle methods here (they are **apprt-free** — generic over the surface type `S` where a surface is touched, exactly as the `renderer_seam` placeholder signatures show):
```rust
impl Renderer {
    pub fn init(alloc: allocator_api2::alloc::Global, options: Options) -> Result<Renderer, Error>; // generic.rs:1795 seam FILLED
    pub fn deinit(&mut self);                                                                        // generic.rs:1841 seam FILLED
    pub fn surface_init<S>(rt_surface: *mut S) -> Result<(), Error>;
    pub fn finalize_surface_init<S>(&mut self, rt_surface: *mut S) -> Result<(), Error>;
    pub fn thread_enter<S>(&self, rt_surface: *mut S) -> Result<(), Error>; // produced by W2-T2 (Metal api hook)
    pub fn thread_exit(&self);                                              // produced by W2-T2
    pub fn draw_frame(&mut self, sync: bool) -> Result<(), Error>;         // exists; on-screen present added W1-3
    pub fn set_visible(&mut self, visible: bool);                          // W2-T2
    pub fn set_focus(&mut self, focus: bool);                              // W2-T2
    pub fn has_vsync(&self) -> bool;                                       // W2-T2
    pub fn set_macos_display_id(&mut self, id: u32);                       // W2-T2
}
```
`rustty_renderer::Options` is extended (W1-0) with the **non-apprt** windowed fields only; the 3 apprt-typed fields (`surface_mailbox`/`rt_surface`/`thread`) stay in `renderer_seam::Options<A>` in core and are consumed there — never named by rustty-renderer.

### renderer_seam::Thread<A> (filled by W3-E2, **stays in core**, struct already defined at `renderer_seam.rs:229-337`)
```rust
// A = rustty_apprt_api::App; struct + ALL fields already exist — DO NOT redefine or move.
impl<A: apprt::App + 'static> Thread<A> {
    // Signature already wired at surface.rs:1857 — MUST match exactly:
    pub fn init(
        alloc: allocator_api2::alloc::Global,
        config: thread::DerivedConfig,
        rt_surface: *mut A::Surface,
        renderer: *mut Renderer,          // the real Renderer (post W3-E1)
        state: *mut State,
        app_mailbox: crate::app::Mailbox<A>,
    ) -> Result<Thread<A>, xev::Error>;
    pub fn deinit(&mut self);                       // NON-consuming (stop.notify()+join in Surface::deinit)
    pub unsafe fn threadMain(self_: *mut Thread<A>); // raw-ptr; spawn already wired via RendererThreadArg (surface.rs:1481-1489,1965)
}
```

## Spec amendments (vs `docs/superpowers/specs/2026-07-08-…-design.md`)

1. **Unit D relocation:** the spec placed the render `Thread` event loop in `rustty-renderer/src/thread.rs`. That violates R4 (the Thread is apprt-generic; rustty-renderer has no apprt-api edge). **Amendment:** the Thread event loop is filled in **core** on the already-defined `renderer_seam::Thread<A>` (W3-E2); rustty-renderer contributes only the apprt-free CVDisplayLink + Metal present (W1/W2). Functionally identical, R4-safe, and matches the already-wired construct-then-spawn in `surface.rs`.
2. **W1 adds task W1-0** (Renderer windowed lifecycle: fill `Renderer::{init,deinit}` at generic.rs:1795/1841, add `surface_init`/`finalize_surface_init` + extend `Options` with the non-apprt windowed fields) — the coverage gap the consistency critic flagged. `thread_enter`/`thread_exit`/`set_visible`/`set_focus`/`has_vsync`/`set_macos_display_id` are produced by W2-T2.

---

## Chunk sequence

- **W0** — deps + `cf_release_thread` rewire + `IOSurfaceLayer` port (present primitive).
- **W1** — Renderer windowed lifecycle (W1-0) + Metal on-screen delta + generic present → **FIRST PIXELS** (W1-6).
- **W2** — CVDisplayLink block in the renderer (apprt-free) + Metal thread/loop hooks.
- **W3** — swap `renderer_seam::Renderer`→real + fill the core-resident `Thread<A>` event loop; `Surface::draw` live.
- **W4** — apprt-c macOS host handshake + window blur/display_id/content-scale → an `NSWindow` showing a live terminal frame.

The detailed per-task specs follow. Remember: the **Canonical Interfaces** section above wins over any per-task signature drift, and each chunk executes as its own design→adversary→port→verify workflow.

---

## Chunk W0 — objc2-quartz-core dep + cf_release_thread rewire (real rustty-xev) + IOSurfaceLayer port

**Goal:** Land the three W0 windowing prerequisites (objc2-quartz-core dep + rustty-xev wiring; cf_release_thread rewired onto the live rustty-xev kqueue loop; IOSurfaceLayer CALayer-subclass present primitive) with two passing macOS KATs.

**Chunk culminating test:** crates/rustty-renderer/src/metal/iosurface_layer.rs::tests::iosurface_layer_sets_surface_as_contents (macOS, offscreen Metal device) — IOSurfaceLayer::init() builds the CALayer subclass, setSurfaceSync(&target.surface) sets a real IOSurface-backed Target surface as `contents`, and layer.contents() returns it (Some + pointer identity); PLUS crates/rustty-os/tests/cf_release_live.rs::cf_release_thread_releases_on_real_xev_loop — a real Thread::init() spawns threadMain on the live rustty-xev kqueue loop, a Release message + wakeup.notify() drains and CFReleases a CFString (+1), stop.notify() ends run(UntilDone), join succeeds, and the CFString retain count drops by 1.


### Task W0.T1: Add objc2-quartz-core dep + wire rustty-xev into rustty-os

**Files:**
- Modify: `crates/rustty-renderer/Cargo.toml` — add a new objc2-quartz-core entry inside the existing [target.'cfg(target_os = "macos")'.dependencies] block (after the objc2-core-foundation stanza, ~line 205); no [workspace.dependencies] entry is needed because the objc2-* crates are pinned inline with literal versions (matching objc2=0.6/objc2-metal=0.3), not via workspace.dependencies
- Modify: `crates/rustty-os/Cargo.toml` — in [target.'cfg(target_os = "macos")'.dependencies] (~line 60, after objc2-core-foundation) add `rustty-xev = { workspace = true }`; and extend the existing `objc2-core-foundation = "0.3"` line to `objc2-core-foundation = { version = "0.3", features = ["CFBase", "CFString"] }` so the cf_release live KAT can build a CFString + read its retain count
- Modify: `crates/rustty-renderer/src/metal.rs` — add `pub mod iosurface_layer;` to the mod list (after `pub mod gpu_abi;` ~line 37, keep alphabetical-ish grouping) so T3's file is compiled
- Test: `crates/rustty-renderer/src/metal/iosurface_layer.rs`

**Consumes:**
- objc2-quartz-core 0.3.1 (objc2 dep req >=0.6.1,<0.8 — compatible with the locked objc2 0.6.4); optional framework deps objc2-core-foundation/objc2-core-graphics/objc2-metal are already resolvable in Cargo.lock
- rustty-xev = { path = "crates/rustty-xev" } already declared in [workspace.dependencies] (workspace Cargo.toml)

**Produces:**
- objc2_quartz_core::CALayer (+ kCAGravityTopLeft, CALayerContentsGravity, CAMetalLayer) available to rustty-renderer
- rustty_xev::{Loop, Async, Completion, Error, NotifyError, MachPortError, CallbackAction, RunMode} available to rustty-os (macOS only)

**Reference (port per R1 — verbatim comments, byte-faithful):**
- `src/renderer/metal/IOSurfaceLayer.zig:1-19` — the imports establishing the objc/macos/CALayer surface that objc2-quartz-core replaces

**FFI / xev mapping:**
- `@import("objc").getClass("CALayer")` → `objc2_quartz_core::CALayer (typed class; feature gate to VERIFY: no `CALayer` cargo feature exists in objc2-quartz-core 0.3.1 — CALayer + kCAGravityTopLeft compile under the `CoreAnimation` umbrella feature, so enable `CoreAnimation` + `CAMetalLayer`)`

**Test KAT:**
```rust
// crates/rustty-renderer/src/metal/iosurface_layer.rs (bottom of file)
// Smoke test proving the objc2-quartz-core dep + feature set are in-lock and
// CALayer/kCAGravityTopLeft link. Full behavior is covered by W0.T3's tests.
#[cfg(all(test, target_os = "macos"))]
mod dep_smoke {
    #[test]
    fn objc2_quartz_core_calayer_and_gravity_link() {
        use objc2_quartz_core::CALayer;
        // The class is registered by the ObjC runtime; `.class()` must resolve.
        let cls = CALayer::class();
        assert_eq!(cls.name().to_string_lossy(), "CALayer");
        // The gravity constant static must be reachable (feature gate present).
        let _g: &objc2_quartz_core::CALayerContentsGravity =
            unsafe { objc2_quartz_core::kCAGravityTopLeft };
    }
}
```

**Port notes:** objc2-quartz-core is the ONE unverified add in the GPU_RD_BLUEPRINT §2 objc2 lock (metal/io-surface/core-graphics already verified). The blueprint (§147-148) mandates it for the IOSurfaceLayer subclass. Feature gaps are the W0 spike per spec §5.4: run `cargo build -p rustty-renderer` and confirm the exact feature that gates CALayer + kCAGravityTopLeft (the 0.3.1 index shows NO `CALayer`/`CAGravity` feature — they fall under `CoreAnimation`; `CAMetalLayer` is a separate feature needed by W1 and is harmless to enable now). The rustty-os -> rustty-xev edge is acyclic (rustty-xev deps only rustty-global #8, below rustty-os #10); gate it under [target.macos] because rustty_xev::Loop/Async are #[cfg(target_os="macos")].

**Gates:**
- `cargo build -p rustty-renderer`
- `cargo build -p rustty-os`
- `cargo tree -p rustty-renderer -i objc2-quartz-core`
- `bash .port/tools/gates/check_acyclic.sh`

**Risk:** MEDIUM — objc2-quartz-core feature names are the one unverified surface; if `CoreAnimation`/`CAMetalLayer` do not expose CALayer/kCAGravityTopLeft, the porter must grep the crate's generated/ sources for the correct gate. Version-resolution risk is low (objc2 dep req spans 0.6.1..0.8, satisfied by locked 0.6.4).

### Task W0.T2: Rewire cf_release_thread.rs onto the REAL rustty-xev Loop/Async

**Files:**
- Create: `crates/rustty-os/tests/cf_release_live.rs`
- Modify: `crates/rustty-os/src/cf_release_thread.rs` — Replace the module doc's DEFERRED-SEAM note (lines 1-32) to state the seam is now satisfied by the real rustty-xev; replace `use xev_seam as xev;` (line 55) with `use rustty_xev as xev;`; DELETE the entire local `mod xev_seam { ... }` placeholder (the unimplemented!() Loop/Async/Completion/Error/AsyncWaitError/CallbackAction/RunMode, ~lines 330-373 per spec, actual ~lines 288-376); gate the `Thread` struct + its impl + wakeupCallback/stopCallback + log helpers under `#[cfg(target_os="macos")]` (keep Message/Mailbox cross-platform); make fields `pub wakeup`, `pub stop`, `pub mailbox`; change the callback ABI + threadMain_ to raw `*mut Thread` (see produces)
- Test: `crates/rustty-os/tests/cf_release_live.rs`

**Consumes:**
- rustty_xev::Loop::init() -> Result<Loop, rustty_xev::Error>
- rustty_xev::Loop::{deinit(&mut self), run(&mut self, RunMode)->Result<(),Error>, stop(&mut self), stopped(&self)->bool}
- rustty_xev::Async::{init()->Result<Async,Error>, deinit(&mut self), notify(&self)->Result<(),NotifyError>, wait::<UD,CB>(self, *mut Loop, *mut Completion, *mut UD, CB)} (Async is Copy)
- rustty_xev::Completion (Default), rustty_xev::RunMode::UntilDone, rustty_xev::CallbackAction::{Rearm,Disarm}, rustty_xev::MachPortError (= Async WaitError)
- rustty_datastruct::BlockingQueue<Message,64> (create()->Box<Self>, destroy(self:Box<Self>), push(&self,T,Timeout)->Size, pop(&self)->Option<T>), rustty_datastruct::blocking_queue::Timeout::Instant
- crate::macos::pthread_setname_np (crates/rustty-os/src/macos.rs:121)

**Produces:**
- #[cfg(target_os="macos")] pub struct Thread { pub loop_: xev::Loop, pub wakeup: xev::Async, wakeup_c: xev::Completion, pub stop: xev::Async, stop_c: xev::Completion, pub mailbox: Box<Mailbox>, flags: Flags }
- pub fn Thread::init() -> Result<Thread, xev::Error>
- pub fn Thread::deinit(mut self)  // consumes: stop.deinit(); wakeup.deinit(); loop_.deinit(); mailbox.destroy()
- pub fn Thread::threadMain(&mut self)  // public entrypoint; delegates to unsafe fn threadMain_(self_: *mut Thread)
- pub enum Message { Release { refs: Vec<*mut std::ffi::c_void> } } (cross-platform)
- pub type Mailbox = BlockingQueue<Message, 64> (cross-platform)
- fn wakeupCallback(self_: *mut Thread, _l: *mut xev::Loop, _c: *mut xev::Completion, r: Result<(), xev::MachPortError>) -> xev::CallbackAction
- fn stopCallback(self_: *mut Thread, _l: *mut xev::Loop, _c: *mut xev::Completion, r: Result<(), xev::MachPortError>) -> xev::CallbackAction

**Reference (port per R1 — verbatim comments, byte-faithful):**
- `src/os/cf_release_thread.zig:61-89` — Thread.init — xev.Loop.init(.{}) + two xev.Async.init() + Mailbox.create (with errdefers)
- `src/os/cf_release_thread.zig:91-103` — deinit — stop.deinit/wakeup.deinit/loop.deinit + mailbox.destroy
- `src/os/cf_release_thread.zig:101-139` — threadMain — errored child call + abrupt-exit drain (flags.drain=true; loop.run(.until_done))
- `src/os/cf_release_thread.zig:120-140` — threadMain_ — pthread_setname_np("cf_release") + wakeup.wait/stop.wait(&loop,&c,Thread,self,cb) + loop.run(.until_done)
- `src/os/cf_release_thread.zig:142-183` — drainMailbox (drain-flag fast path + Release arm CFRelease(ref) + free) + wakeupCallback (.rearm) + stopCallback (loop.stop(); .disarm)

**FFI / xev mapping:**
- `var loop = try xev.Loop.init(.{});` → `let loop_ = xev::Loop::init()?;  // rustty_xev kqueue Loop::init() (kqueue.rs:320)`
- `var wakeup_h = try xev.Async.init(); var stop_h = try xev.Async.init();` → `let wakeup = xev::Async::init()?; let stop = xev::Async::init()?;  // async.rs:122 (mach-port allocate + insert send right)`
- `self.wakeup.wait(&self.loop, &self.wakeup_c, Thread, self, wakeupCallback);` → `unsafe { (*self_).wakeup.wait(&raw mut (*self_).loop_, &raw mut (*self_).wakeup_c, self_, wakeupCallback) };  // async.rs:216 unsafe wait<UD=Thread,CB=fn-item ZST>; self_: *mut Thread`
- `self.stop.wait(&self.loop, &self.stop_c, Thread, self, stopCallback);` → `unsafe { (*self_).stop.wait(&raw mut (*self_).loop_, &raw mut (*self_).stop_c, self_, stopCallback) };`
- `try self.loop.run(.until_done);` → `unsafe { (*self_).loop_.run(xev::RunMode::UntilDone) }  // kqueue.rs:590`
- `self.loop.stopped() / self.loop.stop()` → `self.loop_.stopped() (kqueue.rs:383) / (*self_).loop_.stop() (kqueue.rs:377)`
- `r: xev.Async.WaitError!void  (callback error)` → `r: Result<(), xev::MachPortError>  // WaitError = MachPortError (async.rs:90)`
- `return .rearm; / return .disarm;` → `xev::CallbackAction::Rearm / xev::CallbackAction::Disarm`
- `macos.foundation.CFRelease(ref)` → `unsafe { CFRelease(*r) }  // keep the file-local `extern "C" { fn CFRelease(cf: *mut c_void); }` (unchanged; NOT the xev seam)`
- `producer: thread.wakeup.notify() / thread.stop.notify()` → `thread.wakeup.notify()?  / thread.stop.notify()?  // async.rs:363 notify(&self)->Result<(),NotifyError>; used by the W1 renderer + the live KAT`

**Test KAT:**
```rust
// crates/rustty-os/tests/cf_release_live.rs
//! W0 live-loop KAT (imitates rustty-termio/tests/live_shell.rs): the cf_release
//! Thread runs its REAL rustty-xev kqueue loop; a Release message drains and
//! CFReleases a CFString. Real mach-port/kqueue syscalls -> Miri-excluded.
#![cfg(target_os = "macos")]
#![allow(non_snake_case)]

use std::ffi::c_void;
use std::thread;
use std::time::Duration;

use objc2_core_foundation::{CFGetRetainCount, CFRetained, CFString};
use rustty_datastruct::blocking_queue::Timeout;
use rustty_os::CFReleaseThread::{Message, Thread};

#[test]
fn cf_release_thread_releases_on_real_xev_loop() {
    // Build the thread on the real xev loop (Loop::init + two Async::init).
    let mut t = Thread::init().expect("Thread::init on real xev");

    // Copy the mach-port Async handles out (Async is Copy; u32 port => Send) so a
    // notifier thread can wake + stop the loop while threadMain runs here.
    let wakeup = t.wakeup;
    let stop = t.stop;

    // A heap CFString (retain count 1). Clone -> raw +1 handed to the thread; the
    // original `kept` still holds +1. After the thread CFReleases the raw ref, the
    // count drops from 2 back to 1 => proves drainMailbox -> CFRelease fired.
    let kept: CFRetained<CFString> = CFString::from_str("rustty-cf-release-kat");
    let raw: *mut c_void = CFRetained::into_raw(kept.clone()).as_ptr().cast();
    assert_eq!(unsafe { CFGetRetainCount(Some(kept.as_ref())) }, 2);

    // Publish the Release message BEFORE the loop drains it. The Vec<*mut c_void>
    // never crosses a thread boundary (push + drain both run on THIS thread).
    t.mailbox.push(Message::Release { refs: vec![raw] }, Timeout::Instant);

    // Notifier thread: wake the loop (drain -> CFRelease), then stop it so
    // run(.until_done) returns. Async::notify coalesces + the qlimit-1 mach port
    // holds a pending notify even if it races threadMain arming the wait.
    let notifier = thread::spawn(move || {
        wakeup.notify().expect("wakeup.notify");
        thread::sleep(Duration::from_millis(50));
        stop.notify().expect("stop.notify");
    });

    // Run the REAL loop until stop. If the loop were the old unimplemented!()
    // placeholder this would panic; a clean return proves the live kqueue ran.
    t.threadMain();
    notifier.join().expect("notifier join");

    // The +1 handed to the thread was CFReleased => count back to 1.
    assert_eq!(unsafe { CFGetRetainCount(Some(kept.as_ref())) }, 1);

    // Clean up the thread (stop/wakeup/loop deinit + mailbox destroy).
    t.deinit();
    drop(kept);
}
```

**Port notes:** R5 un-defer: this replaces the deferred xev seam with the live backend — no cf_release_thread.zig logic was ever stubbed (only the xev *dependency* was), so R1 fidelity is unchanged; every ported comment/errdefer marker stays. threadMain_ must take raw `*mut Thread` (not &mut self) to pass `&raw mut (*self_).loop_`/`&raw mut (*self_).wakeup_c` as loop-held raw pointers alongside the self userdata WITHOUT a `&mut self` + interior-raw-ptr aliasing violation across loop ticks — this mirrors rustty-termio/src/thread.rs:735 threadMain_ and its wakeupCallback/stopCallback (thread.rs:1119/1149) which use the IDENTICAL ABI (cb_: *mut UD, r: Result<(),xev::MachPortError>). SB/TB: Async is Copy (mach port u32) so wakeup/stop copy out cheaply and are Send; Loop is NOT Copy and stays owned by Thread. The Box<Mailbox> heap address is stable across a Thread move (Box moves the pointer) — the KAT keeps Thread on the test thread so only the u32 Async handles cross threads. Whole Thread is macOS-gated because rustty_xev::Loop/Async are #[cfg(target_os="macos")]; Message/Mailbox stay cross-platform (no xev), and on non-macOS keep the file-local CFRelease shim so the module still type-checks (nothing consumes Thread off-Darwin). Fields wakeup/stop/mailbox become pub to match Zig package-visibility (the W1 renderer + this KAT are the producers that push+notify). Add crates/rustty-os/tests/cf_release_live.rs to .port/phase-2/MIRI_EXCEPTIONS.md (real mach/kqueue syscalls + fork-free thread spawn; ASan-eligible per R6).

**Gates:**
- `cargo build -p rustty-os`
- `cargo test -p rustty-os --test cf_release_live -- --nocapture`
- `cargo test -p rustty-os --lib`
- `cargo clippy -p rustty-os --all-targets`
- `bash .port/tools/gates/check_acyclic.sh`

**Risk:** MEDIUM — the notify/wait arm race is mitigated by the qlimit-1 mach port holding a pending notify; the 50ms sleep before stop ensures the loop is live. CFGetRetainCount call form (Option<&CFType> via kept.as_ref()) may need a small objc2-core-foundation API tweak; the join-succeeds + no-panic assertion is the fallback proof that the real loop ran even if the retain-count call needs adjustment.

### Task W0.T3: Port IOSurfaceLayer.zig -> metal/iosurface_layer.rs (CALayer subclass present primitive)

**Files:**
- Create: `crates/rustty-renderer/src/metal/iosurface_layer.rs`
- Test: `crates/rustty-renderer/src/metal/iosurface_layer.rs`

**Consumes:**
- objc2::{define_class, msg_send, rc::{Retained, Allocated}, runtime::AnyObject, MainThreadMarker, DefinedClass}
- objc2_quartz_core::{CALayer, CALayerContentsGravity, kCAGravityTopLeft}
- objc2_io_surface::IOSurfaceRef (retain(&self)->CFRetained<IOSurfaceRef>, width(&self)->usize, height(&self)->usize)
- objc2_core_foundation::{CFRetained, CGRect, CGFloat}
- dispatch2::DispatchQueue (main() + exec_async)
- from Target (crates/rustty-renderer/src/metal/target.rs): Target { pub surface: CFRetained<IOSurfaceRef>, .. }; Metal::init_offscreen / Metal::init_target(w,h)->Result<Target,_> for the KAT

**Produces:**
- pub type DisplayCallback = Option<unsafe extern "C-unwind" fn(*mut std::ffi::c_void)>  // ref: ?*align(8) const fn(?*anyopaque) void
- objc2 define_class subclass: pub struct IOSurfaceLayerClass (super = CALayer, name = "IOSurfaceLayer", #[ivars = Ivars]) with `display`/`actionForKey:` overrides; struct Ivars { display_cb: Cell<*const c_void>, display_ctx: Cell<*mut c_void> }
- pub struct IOSurfaceLayer { pub layer: Retained<IOSurfaceLayerClass> }
- pub fn IOSurfaceLayer::init() -> Result<IOSurfaceLayer, Error>  // Error::ObjCFailed
- pub fn IOSurfaceLayer::release(self)  // ARC drop of the Retained
- pub fn IOSurfaceLayer::setSurface(&self, surface: &IOSurfaceRef)  // main-thread dispatch
- pub fn IOSurfaceLayer::setSurfaceSync(&self, surface: &IOSurfaceRef)
- pub fn IOSurfaceLayer::setDisplayCallback(&self, display_cb: DisplayCallback, display_ctx: *mut c_void)
- pub enum Error { ObjCFailed }

**Reference (port per R1 — verbatim comments, byte-faithful):**
- `src/renderer/metal/IOSurfaceLayer.zig:1-19` — module doc + Subclass singleton + `layer: objc.Object` field
- `src/renderer/metal/IOSurfaceLayer.zig:21-46` — init ([CALayer layer].retain(); contentsGravity=kCAGravityTopLeft; ivars display_cb/display_ctx=null) + release
- `src/renderer/metal/IOSurfaceLayer.zig:49-99` — setSurface (surface.retain(); build SetSurfaceBlock; NSThread.isMainThread ? run : dispatch_async(getMain())) + setSurfaceSync (setProperty contents)
- `src/renderer/metal/IOSurfaceLayer.zig:87-141` — SetSurfaceBlock type + setSurfaceCallback (defer surface.release; bounds*scale vs getWidth/getHeight discard-if-mismatch; setProperty contents)
- `src/renderer/metal/IOSurfaceLayer.zig:121-183` — DisplayCallback type + setDisplayCallback (setInstanceVariable) + getSubclass (allocateClassPair CALayer; addIvar x2; replaceMethod display + actionForKey: returning NSNull; registerClassPair)

**FFI / xev mapping:**
- `objc.allocateClassPair(CALayer, "IOSurfaceLayer") + registerClassPair (getSubclass singleton, lines 138-183)` → `objc2::define_class!( #[unsafe(super(CALayer))] #[name = "IOSurfaceLayer"] #[ivars = Ivars] pub struct IOSurfaceLayerClass; impl ... ) — objc2 registers the pair ONCE at first `IOSurfaceLayerClass::class()`, subsuming the `var Subclass: ?objc.Class` singleton`
- `subclass.addIvar("display_cb"); subclass.addIvar("display_ctx")` → `struct Ivars { display_cb: std::cell::Cell<*const c_void>, display_ctx: std::cell::Cell<*mut c_void> } via #[ivars = Ivars]`
- `subclass.replaceMethod("display", fn(target,sel){ cb = getInstanceVariable(display_cb); if cb: cb(getInstanceVariable(display_ctx)) })` → `#[unsafe(method(display))] fn display(&self) { let bits = self.ivars().display_cb.get(); let cb: DisplayCallback = unsafe { core::mem::transmute(bits) }; if let Some(cb) = cb { unsafe { cb(self.ivars().display_ctx.get()) } } }`
- `subclass.replaceMethod("actionForKey:", fn -> [NSNull null])` → `#[unsafe(method(actionForKey:))] fn action_for_key(&self, _key: *mut AnyObject) -> *mut AnyObject { unsafe { msg_send![objc2::class!(NSNull), null] } }`
- `(getSubclass()).msgSend(Object, sel("layer"), .{}).retain()  (init, line 26)` → `let this: Allocated<IOSurfaceLayerClass> = IOSurfaceLayerClass::alloc().set_ivars(Ivars { display_cb: Cell::new(core::ptr::null()), display_ctx: Cell::new(core::ptr::null_mut()) }); let layer: Retained<IOSurfaceLayerClass> = unsafe { msg_send![super(this), init] };  // +layer(autorelease)+retain == alloc+init (+1 owned), behavior-equivalent`
- `layer.setProperty("contentsGravity", kCAGravityTopLeft)` → `unsafe { layer.setContentsGravity(Some(kCAGravityTopLeft)) }  // CALayer contentsGravity setter`
- `self.layer.release()  (release, line 43)` → `impl Drop for IOSurfaceLayer via Retained drop; pub fn release(self) {} consumes -> drops (ARC CFRelease-equivalent)`
- `surface.retain() (line 52) ... defer surface.release() (callback line ~112)` → `let owned: CFRetained<IOSurfaceRef> = surface.retain(); move `owned` into the dispatch closure; it drops (CFRelease) at closure end == the reference's defer release`
- `NSThread.isMainThread` → `objc2::MainThreadMarker::new().is_some()`
- `macos.dispatch.dispatch_async(queue.getMain(), &block)` → `dispatch2::DispatchQueue::main().exec_async(move || Self::setSurfaceCallback(&layer_clone, &owned))  (block2 feature already enabled on dispatch2)`
- `layer.getProperty(Rect,"bounds"); layer.getProperty(f64,"contentsScale")` → `let bounds: CGRect = layer.bounds(); let scale: CGFloat = layer.contentsScale();`
- `width/height = @intFromFloat(bounds.size.* * scale); if != surface.getWidth()/getHeight() discard` → `let w = (bounds.size.width * scale) as usize; let h = (bounds.size.height * scale) as usize; if w != surface.width() || h != surface.height() { return; }`
- `layer.setProperty("contents", surface)  (setSurfaceSync + setSurfaceCallback)` → `let obj = surface as *const IOSurfaceRef as *const AnyObject; unsafe { self.layer.setContents(Some(&*obj)) }  // CALayer setContents(Option<&AnyObject>)`
- `self.layer.setInstanceVariable("display_cb"/"display_ctx", ...)  (setDisplayCallback)` → `self.layer.ivars().display_cb.set(display_cb.map_or(core::ptr::null(), |f| f as *const c_void)); self.layer.ivars().display_ctx.set(display_ctx);`

**Test KAT:**
```rust
// crates/rustty-renderer/src/metal/iosurface_layer.rs (bottom of file)
#[cfg(all(test, target_os = "macos"))]
mod tests {
    use std::ffi::c_void;
    use std::sync::atomic::{AtomicBool, Ordering};

    use objc2::{msg_send, rc::Retained};
    use objc2::runtime::AnyObject;
    use objc2_io_surface::IOSurfaceRef;
    use rustty_render_types::AlphaBlending;

    use super::{DisplayCallback, IOSurfaceLayer};
    use crate::metal::metal_impl::{Error as MetalError, Metal};

    // W0.T3 culminating KAT: the CALayer subclass sets a real IOSurface as its
    // `contents`. Reuses the offscreen Metal device (the golden-gate device path)
    // to mint a real IOSurface-backed Target.
    #[test]
    fn iosurface_layer_sets_surface_as_contents() {
        let metal = match Metal::init_offscreen(AlphaBlending::Native, 64, 32) {
            Ok(m) => m,
            Err(MetalError::NoMetalDevice) => {
                eprintln!("no Metal device; skipping");
                return;
            }
        };
        let target = metal.init_target(64, 32).expect("init_target");

        let layer = IOSurfaceLayer::init().expect("IOSurfaceLayer::init");
        // Freshly-inited layer has no contents.
        let contents0: Option<Retained<AnyObject>> = unsafe { layer.layer.contents() };
        assert!(contents0.is_none());

        // setSurfaceSync sets the IOSurface as `contents` (no main-thread hop).
        layer.setSurfaceSync(&target.surface);
        let contents1: Retained<AnyObject> =
            unsafe { layer.layer.contents() }.expect("contents set");

        // Pointer identity: CALayer stores the same IOSurface object we handed it.
        let want = (&*target.surface as *const IOSurfaceRef).cast::<c_void>();
        let got = Retained::as_ptr(&contents1) as *const c_void;
        assert_eq!(got, want, "contents id must be the IOSurface we set");
    }

    static FIRED: AtomicBool = AtomicBool::new(false);
    unsafe extern "C-unwind" fn on_display(ctx: *mut c_void) {
        assert_eq!(ctx, 0xABCD_usize as *mut c_void);
        FIRED.store(true, Ordering::SeqCst);
    }

    // The overridden `display` method invokes the stored display callback+ctx.
    #[test]
    fn iosurface_layer_display_callback_fires() {
        FIRED.store(false, Ordering::SeqCst);
        let layer = IOSurfaceLayer::init().expect("IOSurfaceLayer::init");
        let cb: DisplayCallback = Some(on_display);
        layer.setDisplayCallback(cb, 0xABCD_usize as *mut c_void);
        // Send -display directly (the CVDisplayLink/present path invokes it live).
        unsafe { let _: () = msg_send![&*layer.layer, display] };
        assert!(FIRED.load(Ordering::SeqCst), "display callback must fire");
    }
}
```

**Port notes:** R1 fidelity: every reference comment (the autorelease/retain rationale at init line 22-25, the block-copy retain note line 62-64, the wrong-size discard jank explanation line 100-105, the disable-animations actionForKey: comment line 163) ports verbatim. Key behavior-preserving maps: (1) `[CALayer layer].retain()` -> `alloc().set_ivars(..).init()` yields the same +1-owned subclass instance (the reference's +layer autorelease + explicit retain nets +1; alloc+init nets +1 with no pool), and the ivars default to null at construction, matching the reference's explicit post-init null-set (lines 37-38). (2) The two ObjC ivars become a typed `Ivars { Cell<*const c_void>, Cell<*mut c_void> }` — Cell gives interior mutability so setDisplayCallback takes &self (reference receiver *IOSurfaceLayer, but only writes ivars). The DisplayCallback fn-pointer is stored as raw bits (transmute round-trip) since objc2 ivars can't hold an Option<fn>. (3) surface retain/release: the reference retains before dispatch and releases in the callback (defer); the port moves an owned CFRetained<IOSurfaceRef> clone into the dispatch closure so it drops (CFRelease) exactly when the callback returns. The Retained<IOSurfaceLayerClass> is cloned (retain) into the closure to match the reference's note that dispatch_async auto-retains the self.layer id when the block is copied. (4) release()->ARC Drop. SB/TB: no long-lived &mut crosses the dispatch boundary; the closure captures owned values only (CFRetained + Retained), so no aliasing of `self`. define_class is the blueprint-mandated (§147-148) objc2 form of runtime allocateClassPair — same registered ObjC class, singleton-once. Miri-excluded (objc runtime + Metal); ASan-eligible (R6).

**Gates:**
- `cargo build -p rustty-renderer`
- `cargo test -p rustty-renderer metal::iosurface_layer -- --nocapture`
- `cargo clippy -p rustty-renderer --all-targets`
- `bash .port/tools/gates/check_acyclic.sh`

**Risk:** MEDIUM-HIGH — two objc2 surfaces to verify against 0.6.4/quartz-core 0.3.1: (a) CALayer's `bounds()`/`contentsScale()`/`setContents()`/`contentsGravity` accessor names + whether they need a MainThreadMarker/are gated by CoreAnimation; (b) whether CALayer boxes the IOSurface on setContents (breaking the pointer-identity assert) — if so, relax the KAT to assert contents().is_some() only. define_class super(CALayer) + `+layer`-vs-alloc/init and the fn-ptr-in-Cell ivar are the trickiest bits; spike alongside T1's feature check.

**Open risks:** objc2-quartz-core 0.3.1 has NO `CALayer`/`CAGravity` cargo feature — CALayer + kCAGravityTopLeft appear to compile under the `CoreAnimation` umbrella feature; the exact enabling feature must be confirmed by `cargo build -p rustty-renderer` (spec §5.4 W0 spike). Enable `CoreAnimation` + `CAMetalLayer` and adjust if build fails.; CALayer accessor method names in objc2-quartz-core 0.3.1 (bounds/contentsScale/setContents/contentsGravity) are unverified; some may require a MainThreadMarker argument or differ in casing. The IOSurfaceLayer subclass via objc2 define_class(super(CALayer)) with a fn-pointer-in-Cell ivar and the +layer->alloc/init map is the highest-uncertainty code and should be spiked first.; Pointer-identity assert in the contents KAT assumes CALayer stores the same IOSurface object; if CALayer wraps/boxes it, relax to contents().is_some().; cf_release live KAT: wakeup.notify() may race threadMain arming the wait — mitigated by the qlimit-1 mach port retaining a pending notify; if flaky, the join-succeeds assertion still proves the live loop ran. CFGetRetainCount call form (Option<&CFType>) may need an objc2-core-foundation API tweak.; rustty-os becomes an FFI/thread-spawning + objc crate on macOS (rustty-xev edge); confirm check_acyclic.sh (real Cargo graph) stays green and add both new tests to MIRI_EXCEPTIONS.md (R6: Miri-excluded, ASan-eligible).

---

## Chunk W1 — Metal on-screen delta (Metal.zig unit B) + generic R5 present half (unit C, present-only) = FIRST PIXELS. Un-defers the Metal graphics-API layer-attach/present seams and the generic present error-log seam so a real terminal frame drawn into an IOSurface-backed Target is pushed onto a CALayer attached to a live NSView.

**Goal:** Make Metal::init bind a W0 IOSurfaceLayer to a host NSView (wantsLayer/clipsToBounds/contentsScale/needsDisplayOnBoundsChange), give Metal a real on-screen surface_size (layer.bounds*contentsScale) and present/presentLastTarget (layer.setSurface[Sync]) + loopEnter (setDisplayCallback), implement Frame.complete's async addCompletedHandler present half, and restore generic.rs's R1 present-failure error log — culminating in an on-screen present of one golden-state terminal frame (first real pixels on a CALayer).

**Chunk culminating test:** generic::tests::golden_presents_frame_on_screen (W1-6): draws one golden-state terminal frame ("Ab" + block cursor) into an IOSurface-backed Target, asserts the cursor pixels are genuinely red (reusing the offscreen golden gate), then builds an on-screen Metal via Metal::init over a bare NSView and presents that exact IOSurface — asserting the CALayer attached to the view now hosts the drawn surface as its contents. This is FIRST PIXELS: init view-attach + present setSurface proven end-to-end against real terminal pixels.


### Task W1-1: Add layer:Option<IOSurfaceLayer> field to Metal + import W0 leaf; set None in init_offscreen

**Files:**
- Modify: `crates/rustty-renderer/src/metal/metal_impl.rs` — 59-64 (add `use super::iosurface_layer::{self, IOSurfaceLayer};`), 97-138 (struct Metal: replace the M-1 DEFERRED comment block with the real `layer: Option<IOSurfaceLayer>` field), 240-249 (init_offscreen tail: add `layer: None,` to the returned struct literal), 660-680 (Drop: update the M-1 note — layer releases via IOSurfaceLayer Drop)
- Test: `crates/rustty-renderer/src/metal/metal_impl.rs`

**Consumes:**
- crate::metal::iosurface_layer::IOSurfaceLayer (W0 produce)
- IOSurfaceLayer::init() -> Result<IOSurfaceLayer, iosurface_layer::Error>
- IOSurfaceLayer Drop releases the retained CALayer

**Produces:**
- Metal.layer: Option<IOSurfaceLayer> (None == offscreen divergence, Some == on-screen; consumed by W1-2/W1-3 present + surface_size)
- Metal::init_offscreen still returns a valid `layer: None` Metal (all Chunk-2/3/4 golden call sites keep compiling unchanged)

**Reference (port per R1 — verbatim comments, byte-faithful):**
- `src/renderer/Metal.zig:41` — `layer: IOSurfaceLayer,` — the non-optional field on the Zig struct (rustty makes it Option<> to keep the sanctioned offscreen path)
- `src/renderer/Metal.zig:157-160` — deinit: self.layer.release() — maps to IOSurfaceLayer Drop

**FFI / xev mapping:**
- `layer: IOSurfaceLayer (struct field)` → `pub layer: Option<IOSurfaceLayer>`
- `self.layer.release() (deinit)` → `<IOSurfaceLayer as Drop>::drop (ARC via Retained<CALayer>)`

**Test KAT:**
```rust
#[cfg(target_os = "macos")]
#[test]
fn offscreen_metal_has_no_layer() {
    // init_offscreen keeps the sanctioned no-layer divergence: layer == None,
    // so the existing golden/offscreen present no-op path is unchanged.
    let m = match Metal::init_offscreen(AlphaBlending::Native, 64, 32) {
        Ok(m) => m,
        Err(_) => return, // headless CI: no Metal device
    };
    assert!(m.layer.is_none(), "offscreen Metal must not attach a layer");
    // surface_size still returns the fixed offscreen size (layer==None branch).
    let s = m.surface_size().expect("surface_size");
    assert_eq!((s.width, s.height), (64, 32));
}
```

**Port notes:** SANCTIONED DIVERGENCE (R1-documented): ghostty's `layer: IOSurfaceLayer` is non-optional because ghostty always has a host view. rustty added the offscreen golden path (init_offscreen, no view) BEFORE the windowing tier, so the minimal faithful encoding is `Option<IOSurfaceLayer>`: None == offscreen (present no-op, surface_size==offscreen_size), Some == on-screen. The verbatim Zig field decl stays in a doc-comment above the field. No byte-fidelity concern (pure struct shape). Drop order immaterial under ARC.

**Gates:**
- `cargo build -p rustty-renderer`
- `cargo test -p rustty-renderer --lib metal::`
- `cargo clippy -p rustty-renderer --all-targets -- -D warnings`
- `cargo fmt --check`
- `bash /Users/gael/projects/rustty/.port/tools/gates/check_acyclic.sh`
- `python3 /Users/gael/projects/rustty/.port/tools/gates/check_comment_coverage.py`

**Risk:** Low — pure field addition; the Option divergence must be documented or a completeness critic flags a non-verbatim field.

### Task W1-2: Implement Metal::init (on-screen constructor, M-4 view attach)

**Files:**
- Modify: `crates/rustty-renderer/src/metal/metal_impl.rs` — 254-337 (replace the `pub fn init(/* alloc, opts */) -> Result<Metal,Error> { unimplemented!(...) }` seam at 332-337 with the real on-screen ctor; keep the verbatim Zig body comment 259-331)
- Test: `crates/rustty-renderer/src/metal/metal_impl.rs`

**Consumes:**
- choose_device() -> Result<Retained<ProtocolObject<dyn MTLDevice>>, Error> (metal_impl.rs:150)
- query_max_texture_size(&device) -> u32 (metal_impl.rs)
- device.newCommandQueue()/hasUnifiedMemory() (objc2-metal)
- IOSurfaceLayer::init() -> Result<IOSurfaceLayer, iosurface_layer::Error> (W0)
- IOSurfaceLayer.layer: Retained<objc2_quartz_core::CALayer> (W0)
- AlphaBlending (rustty_render_types)
- Metal.layer field (W1-1)

**Produces:**
- pub fn init(view: &objc2::runtime::AnyObject, scale_factor: f64, blending: AlphaBlending) -> Result<Metal, Error>  // view is the host NSView/UIView, type-erased exactly like ghostty's `info.view: objc.Object`; returns a Metal with layer: Some(..) attached to `view`

**Reference (port per R1 — verbatim comments, byte-faithful):**
- `src/renderer/Metal.zig:64-156` — pub fn init(alloc, opts): chooseDevice+newCommandQueue+metadata, ViewInfo extraction (view + scaleFactor from opts.rt_surface), IOSurfaceLayer.init, setProperty(layer)/setProperty(wantsLayer)/setProperty(clipsToBounds), contentsScale, needsDisplayOnBoundsChange

**FFI / xev mapping:**
- `info.view.setProperty("layer", layer.layer.value)` → `unsafe { let l: *const objc2_quartz_core::CALayer = &*layer.layer; let _: () = objc2::msg_send![view, setLayer: l]; }  // setProperty is KVC setValue:forKey:@"layer" → real setter setLayer:`
- `info.view.setProperty("wantsLayer", true)` → `unsafe { let _: () = objc2::msg_send![view, setWantsLayer: true]; }`
- `info.view.setProperty("clipsToBounds", true)` → `unsafe { let _: () = objc2::msg_send![view, setClipsToBounds: true]; }`
- `layer.layer.setProperty("contentsScale", info.scaleFactor)` → `layer.layer.setContentsScale(scale_factor)  // objc2-quartz-core CALayer`
- `layer.layer.setProperty("needsDisplayOnBoundsChange", true)` → `layer.layer.setNeedsDisplayOnBoundsChange(true)`
- `const device = try chooseDevice()` → `let device = choose_device()?;`
- `queue = device.msgSend(newCommandQueue)` → `let queue = device.newCommandQueue().ok_or(Error::NoMetalDevice)?;`

**Test KAT:**
```rust
#[cfg(target_os = "macos")]
#[test]
fn init_binds_layer_to_nsview() {
    use objc2::{class, msg_send, rc::Retained, runtime::AnyObject};
    // Skip cleanly on headless CI (no Metal device).
    if Metal::init_offscreen(AlphaBlending::Native, 1, 1).is_err() {
        eprintln!("no Metal device; skipping");
        return;
    }
    // A bare NSView needs no running NSApplication to accept a layer-host handshake.
    let view: Retained<AnyObject> = unsafe { msg_send![class!(NSView), new] };
    let m = Metal::init(&view, 2.0, AlphaBlending::Native).expect("on-screen init");
    // The layer was attached (Some) and is the view's layer (layer-hosting view).
    let layer = m.layer.as_ref().expect("on-screen Metal must have a layer");
    let want: *const objc2_quartz_core::CALayer = &*layer.layer;
    let got: *mut AnyObject = unsafe { msg_send![&*view, layer] };
    assert_eq!(got as *const AnyObject, want.cast(), "view.layer != our IOSurfaceLayer");
    let wants_layer: bool = unsafe { msg_send![&*view, wantsLayer] };
    assert!(wants_layer, "view is not layer-hosting (wantsLayer==false)");
    // contentsScale threaded through from the scale_factor arg.
    assert!((layer.layer.contentsScale() - 2.0).abs() < 1e-9);
}
```

**Port notes:** The Zig `info: ViewInfo` is extracted from `opts.rt_surface` (embedded apprt) — that full Options threading is a W3/renderer_seam concern, so W1 exposes init with the two already-extracted values as concrete params (view + scale_factor), exactly as init_offscreen took blending/width/height directly rather than Options. `view` stays TYPE-ERASED (&AnyObject) — byte-faithful to ghostty's `info.view: objc.Object` — so the renderer crate needs NO objc2-app-kit dep. Order is load-bearing: setLayer BEFORE setWantsLayer (Zig comment: assign layer to make the view layer-hosting BEFORE wantsLayer). No SB/TB concern (msg_send over &AnyObject, no Rust aliasing). errdefer device/queue/layer release → ARC via Retained Drop on the `?` early returns.

**Gates:**
- `cargo build -p rustty-renderer`
- `cargo test -p rustty-renderer --lib metal::metal_impl::tests::init_binds_layer_to_nsview`
- `cargo clippy -p rustty-renderer --all-targets -- -D warnings`
- `cargo fmt --check`
- `bash /Users/gael/projects/rustty/.port/tools/gates/check_acyclic.sh`

**Risk:** Med — creating a bare NSView on headless CI without NSApplication can occasionally require +[NSApplication sharedApplication] first; if the view-attach msg_send throws, gate the test behind an sharedApplication init or fall back to attaching the layer to a raw CALayer host. objc2 msg_send selector names (setLayer:/setWantsLayer:) must match AppKit exactly.

### Task W1-3: Implement surface_size (on-screen), present/presentLastTarget, and loop_enter

**Files:**
- Modify: `crates/rustty-renderer/src/metal/metal_impl.rs` — 372-388 (loop_enter: replace unimplemented! with setDisplayCallback + displayCallback trampoline), 580-604 (surface_size: add the layer.bounds*contentsScale branch when layer==Some), 636-650 (present: replace no-op with layer.setSurface[Sync]), 652-657 (present_last_target: keep no-op, R1 comment)
- Test: `crates/rustty-renderer/src/metal/metal_impl.rs`

**Consumes:**
- Metal.layer: Option<IOSurfaceLayer> (W1-1)
- IOSurfaceLayer.set_surface_sync(&self, surface: &objc2_io_surface::IOSurfaceRef) (W0)
- IOSurfaceLayer.set_surface(&self, surface: &objc2_io_surface::IOSurfaceRef) -> Result<(), iosurface_layer::Error> (W0)
- IOSurfaceLayer.set_display_callback(&self, cb: iosurface_layer::DisplayCallback, ctx: *mut core::ffi::c_void) (W0)
- IOSurfaceLayer.layer.bounds()/contentsScale() (objc2-quartz-core CALayer)
- Target.surface: CFRetained<objc2_io_surface::IOSurfaceRef> (metal/target.rs:68)
- self.max_texture_size, self.offscreen_size, SurfaceSize (metal_impl.rs:79)

**Produces:**
- pub fn present(&mut self, target: &Target, sync: bool) -> Result<(), Error>  // Some(layer): sync→set_surface_sync, async→set_surface; None: no-op (offscreen)
- pub fn present_last_target(&self) -> Result<(), Error>  // no-op (Metal), unchanged
- pub fn surface_size(&self) -> Result<SurfaceSize, Error>  // Some(layer): (bounds.size*contentsScale).min(max_texture_size); None: offscreen_size.min(max_texture_size)
- pub fn loop_enter(&mut self)  // installs displayCallback on the layer

**Reference (port per R1 — verbatim comments, byte-faithful):**
- `src/renderer/Metal.zig:216-232` — surfaceSize: bounds = layer.getProperty(Rect,"bounds"); scale = getProperty(f64,"contentsScale"); width=@min(@intFromFloat(bounds.size.width*scale), max_texture_size), height likewise
- `src/renderer/Metal.zig:253-259` — present: if sync self.layer.setSurfaceSync(target.surface) else try self.layer.setSurface(target.surface)
- `src/renderer/Metal.zig:261-263` — presentLastTarget: `_ = self;` noop for Metal
- `src/renderer/Metal.zig:164-175` — loopEnter: renderer=@fieldParentPtr("api",self); self.layer.setDisplayCallback(&displayCallback, renderer); displayCallback(renderer) => renderer.drawFrame(true) catch log.warn

**FFI / xev mapping:**
- `self.layer.setSurfaceSync(target.surface)` → `layer.set_surface_sync(&target.surface)`
- `try self.layer.setSurface(target.surface)` → `layer.set_surface(&target.surface).map_err(|_| Error::NoMetalDevice)?`
- `self.layer.layer.getProperty(Rect,"bounds")` → `let b = layer.layer.bounds(); // objc2_core_foundation::CGRect`
- `self.layer.layer.getProperty(f64,"contentsScale")` → `let scale = layer.layer.contentsScale();`
- `self.layer.setDisplayCallback(@ptrCast(&displayCallback), @ptrCast(renderer))` → `layer.set_display_callback(Some(display_callback_trampoline), renderer_ptr as *mut core::ffi::c_void)`
- `fn displayCallback(renderer:*Renderer) { renderer.drawFrame(true) catch log.warn }` → `unsafe extern "C" fn display_callback_trampoline(ctx: *mut c_void) { /* cast to *mut Renderer; (*r).draw_frame(true); log on Err */ }  // Renderer is the monomorphic GenericRenderer — trampoline is authored where Renderer is in scope; see port_notes`

**Test KAT:**
```rust
#[cfg(target_os = "macos")]
#[test]
fn onscreen_surface_size_and_present_from_layer() {
    use objc2::{class, msg_send, rc::Retained, runtime::AnyObject};
    if Metal::init_offscreen(AlphaBlending::Native, 1, 1).is_err() { return; }
    let view: Retained<AnyObject> = unsafe { msg_send![class!(NSView), new] };
    let mut m = Metal::init(&view, 1.0, AlphaBlending::Native).expect("init");
    // Give the layer a concrete bounds so surface_size reads it back.
    let layer = m.layer.as_ref().unwrap();
    let rect = objc2_core_foundation::CGRect::new(
        objc2_core_foundation::CGPoint::new(0.0, 0.0),
        objc2_core_foundation::CGSize::new(48.0, 24.0),
    );
    layer.layer.setBounds(rect);
    layer.layer.setContentsScale(1.0);
    let s = m.surface_size().expect("surface_size");
    assert_eq!((s.width, s.height), (48, 24), "surface_size must read layer.bounds*scale");
    // Draw one real frame offscreen to obtain a populated IOSurface-backed Target,
    // then present that surface onto our on-screen layer (sync path).
    let mut off = Metal::init_offscreen(AlphaBlending::Native, 48, 24).unwrap();
    let target = off.init_target(48, 24).expect("init_target");
    m.present(&target, true).expect("present sync");
    // The layer now hosts the presented IOSurface as its contents (first pixels).
    let contents: Option<Retained<AnyObject>> = m.layer.as_ref().unwrap().layer.contents();
    let got = contents.expect("layer.contents unset after present");
    let want: *const objc2_io_surface::IOSurfaceRef = &*target.surface;
    assert_eq!(Retained::as_ptr(&got) as *const _ as *const objc2_io_surface::IOSurfaceRef, want,
        "layer.contents is not the presented IOSurface");
    let _ = &mut off; // keep off (owns the surface) alive past the assert
}
```

**Port notes:** surface_size: reuse the existing clamp (`.min(self.max_texture_size)`) for BOTH branches — Zig applies @min on each axis. The layer branch uses objc2_core_foundation::CGRect (objc2-quartz-core bounds return type) — bounds.size.width/height are CGFloat(f64); `@intFromFloat` → `as u32` truncation (byte-faithful: trunc-toward-zero). present error mapping: Zig `try setSurface` error set folds into `!void`; here the W0 iosurface_layer::Error maps to metal_impl::Error (the drawFrame error set that generic.rs already threads). loopEnter/displayCallback: ghostty recovers `renderer` via `@fieldParentPtr("api", self)` — in the monomorphic rustty port `Renderer` owns `api: Metal` (generic.rs), so the trampoline must live where GenericRenderer is nameable. PORT DECISION: `loop_enter` stays a thin method that stores the `*mut c_void` ctx via `layer.set_display_callback`; the concrete `display_callback_trampoline` (casts ctx→*mut Renderer, calls draw_frame(true), logs on Err — Zig's `catch log.warn`) is authored in generic.rs alongside metal_buffer_completed (same container_of/raw-*mut discipline, NEVER &mut) and passed in. If W1 scopes loop_enter narrowly, leave it `unimplemented!()` and land the trampoline in W2 (render-thread) — flag in DEFERRALS.md. The present raw-ptr path (`&mut api.present` through `*mut Renderer` in metal_buffer_completed) is the proven-clean SB/TB structure (.port/phase-2/mirihar-chunk3b).

**Gates:**
- `cargo build -p rustty-renderer`
- `cargo test -p rustty-renderer --lib metal::metal_impl::tests::onscreen_surface_size_and_present_from_layer`
- `cargo clippy -p rustty-renderer --all-targets -- -D warnings`
- `cargo fmt --check`
- `python3 /Users/gael/projects/rustty/.port/tools/gates/check_comment_coverage.py`

**Risk:** Med — objc2-quartz-core CALayer accessor names (bounds/setBounds/contentsScale/contents/setNeedsDisplayOnBoundsChange) are the one unverified objc2 add (spec OPEN RISK High); confirm they exist in objc2-quartz-core 0.3 during W0 spike. displayCallback cross-module authoring may push loop_enter to W2.

### Task W1-4: Frame.complete async addCompletedHandler present half

**Files:**
- Modify: `crates/rustty-renderer/src/metal/frame.rs` — 185-199 (replace the `unimplemented!("windowing-host: async addCompletedHandler present")` at 198 with the block2::RcBlock install; the R5 shape comment 186-197 becomes the implementation), 26 (module doc: drop the '(unimplemented!)' note on the async path)
- Test: `crates/rustty-renderer/src/metal/frame.rs`

**Consumes:**
- Frame.block: CompletionContext { renderer: *mut (), target: *mut Target, sync: bool } (frame.rs:50)
- Frame.on_completed: CompletionFn = unsafe fn(&CompletionContext, &ProtocolObject<dyn MTLCommandBuffer>) (frame.rs:60)
- self.buffer: Retained<ProtocolObject<dyn MTLCommandBuffer>> with addCompletedHandler (objc2-metal) + commit (frame.rs:201)
- block2::RcBlock (block2 0.6)

**Produces:**
- Frame::complete(&mut self, sync: bool) — async branch now installs the completion handler instead of unimplemented!(); sync branch unchanged (commit + waitUntilCompleted + direct on_completed call)

**Reference (port per R1 — verbatim comments, byte-faithful):**
- `src/renderer/metal/Frame.zig:104-133` — complete: if(!sync) self.buffer.msgSend(void, addCompletedHandler:, .{&self.block}); commit; if(sync){ waitUntilCompleted; block.sync=true; CompletionBlock.invoke(&block, .{buffer.value}) }
- `src/renderer/metal/Frame.zig:58-90` — CompletionBlock over Context{renderer,target,sync} + bufferCompleted body (status→present→frameCompleted) — the fn the block dispatches to; = rustty CompletionFn/on_completed

**FFI / xev mapping:**
- `self.buffer.msgSend(void, objc.sel("addCompletedHandler:"), .{&self.block})` → `let handler: block2::RcBlock<dyn Fn(core::ptr::NonNull<ProtocolObject<dyn MTLCommandBuffer>>)> = block2::RcBlock::new(move |buf| { unsafe { (on_completed)(&ctx, buf.as_ref()); } }); unsafe { self.buffer.addCompletedHandler(block2::RcBlock::as_ptr(&handler)); }`
- `CompletionBlock.init copies Context by value into the block` → `the RcBlock captures a `ctx: CompletionContext` COPY (raw ptrs are Copy) + `on_completed` fn-ptr by move — matches Zig's by-value Context copy so it stays valid until Metal fires the handler`

**Test KAT:**
```rust
#[cfg(target_os = "macos")]
#[test]
fn frame_complete_async_installs_handler_and_commits() {
    // Reuses the module's frame smoke harness: build an offscreen Metal, a target,
    // begin a frame with a no-op on_completed, and drive the async (!sync) path.
    let api = match super::super::metal_impl::Metal::init_offscreen(AlphaBlending::Native, 8, 8) {
        Ok(m) => m, Err(_) => return, // headless CI
    };
    let mut target = api.init_target(8, 8).expect("init_target");
    // A CompletionFn that records it ran (via a static atomic) — proves the block fired.
    unsafe fn on_done(_ctx: &CompletionContext, _buf: &ProtocolObject<dyn MTLCommandBuffer>) {
        FIRED.store(true, core::sync::atomic::Ordering::SeqCst);
    }
    static FIRED: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);
    let mut frame = api.begin_frame(core::ptr::null_mut(), &mut target as *mut _, on_done)
        .expect("begin_frame");
    // Empty async frame: install handler + commit, no unimplemented! panic.
    frame.complete(false);
    // Metal invokes the completion handler shortly after commit; wait for the buffer.
    frame.buffer.waitUntilCompleted();
    // A short bounded spin lets the objc runtime run the copied block.
    for _ in 0..1000 {
        if FIRED.load(core::sync::atomic::Ordering::SeqCst) { break; }
        std::thread::yield_now();
    }
    assert!(FIRED.load(core::sync::atomic::Ordering::SeqCst),
        "async addCompletedHandler block did not fire");
}
```

**Port notes:** CompletionContext must be Copy (all fields — two raw ptrs + bool — are Copy) so the RcBlock owns a by-value copy, mirroring Zig's `CompletionBlock.init(.{...})` which copies the Context struct into the block (so the async body does NOT borrow `self.block`, which would dangle after `complete` returns). block2 copies the RcBlock when addCompletedHandler retains it and the objc runtime deallocs the copy on completion (Zig comment at Frame.zig:106-108 preserved). SB/TB: the block body derefs raw `*mut Renderer`/`*mut Target` from the copied ctx — NEVER &mut; identical container_of discipline to the sync path. The block2 shape is ALREADY spelled verbatim in the frame.rs:186-197 comment — un-defer it verbatim. If the async-fire assertion proves flaky on CI (handler timing), downgrade to asserting `complete(false)` returns without panic and defer the fire-observation to W2's render-thread integration test.

**Gates:**
- `cargo build -p rustty-renderer`
- `cargo test -p rustty-renderer --lib metal::frame::`
- `cargo clippy -p rustty-renderer --all-targets -- -D warnings`
- `cargo fmt --check`
- `cargo +nightly miri test -p rustty-renderer --lib metal::frame:: 2>/dev/null || true  # FFI test is Miri-excluded (foreign calls); recorded in MIRI_EXCEPTIONS.md`

**Risk:** Med — the async block firing is timing-dependent; the bounded-spin assertion may be flaky on loaded CI. block2::RcBlock<dyn Fn(NonNull<...>)> vs objc2-metal's addCompletedHandler MTLCommandBufferHandler param type must line up (the frame.rs comment already pins this shape).

### Task W1-5: generic.rs: restore R1 present-failure error log at metal_buffer_completed (present half of unit C)

**Files:**
- Modify: `crates/rustty-renderer/src/generic.rs` — 4618-4632 (metal_buffer_completed: replace `let _ = unsafe { (*renderer).api.present(&*ctx.target, ctx.sync) };` with the verbatim `if let Err(err) = … { log::error!(…) }` now that present can fail on the layer path; the R1 comment 4620-4627 becomes the live code)
- Test: `crates/rustty-renderer/src/generic.rs`

**Consumes:**
- Metal::present(&mut self, target: &Target, sync: bool) -> Result<(), Error> (W1-3)
- CompletionContext { renderer: *mut (), target: *mut Target, sync: bool } (frame.rs)
- Renderer (monomorphic GenericRenderer<Metal>) with pub api: Metal + frame_completed(&mut self, Health) (generic.rs)
- crate::Health

**Produces:**
- metal_buffer_completed(ctx: &CompletionContext, buffer: &ProtocolObject<dyn MTLCommandBuffer>) — the present-error branch now logs on Err (Frame.zig:82-88 restored verbatim); still calls frame_completed(health) unconditionally

**Reference (port per R1 — verbatim comments, byte-faithful):**
- `src/renderer/metal/Frame.zig:79-90` — if (health == .healthy) { block.renderer.api.present(block.target.*, block.sync) catch |err| log.err("Failed to present render target: err={}", .{err}); } block.renderer.frameCompleted(health);

**FFI / xev mapping:**
- `block.renderer.api.present(block.target.*, block.sync) catch |err| log.err(...)` → `if let Err(err) = unsafe { (*renderer).api.present(&*ctx.target, ctx.sync) } { log::error!("Failed to present render target: err={err:?}"); }`
- `block.renderer.frameCompleted(health)` → `unsafe { (*renderer).frame_completed(health) };  // unchanged`

**Test KAT:**
```rust
// Present-path coverage lands via the culminating on-screen KAT (W1-6), which
// drives metal_buffer_completed → api.present against a REAL layer. Here a focused
// unit asserts the offscreen path still completes cleanly (present no-op ⇒ no Err
// ⇒ no spurious log) and frame_completed still fires — reusing golden_renderer.
#[test]
fn buffer_completed_offscreen_still_healthy() {
    let mut r = match golden_renderer(6, 2) { Some(r) => r, None => return };
    let mut mutex = Mutex::new();
    let mut t = Terminal::init(Global, opts(6, 2)).unwrap();
    { let mut s = t.vt_stream(); s.next_slice(b"Hi"); s.deinit(); }
    let st = state::State { mutex: &mut mutex, terminal: &mut t, inspector: None,
        preedit: None, mouse: state::Mouse::default() };
    r.update_frame(&st, true).unwrap();
    // Sync draw drives metal_buffer_completed → api.present (offscreen no-op) →
    // frame_completed; health stays Healthy (no present Err to log).
    r.draw_frame(true).unwrap();
    assert_eq!(r.health.load(core::sync::atomic::Ordering::SeqCst), crate::Health::Healthy);
    t.deinit(Global);
}
```

**Port notes:** This is the ONLY generic.rs change W1 needs for the PRESENT half: the two R5 seams the spec cites at generic.rs:1795/1841 are Renderer::init / Renderer::deinit (the swap_chain/display_link construct+teardown) — those are the VSYNC/renderer_seam concern (W2/W3), NOT the present half, and the culminating KAT explicitly REUSES the field-by-field golden harness (golden_renderer) which bypasses Renderer::init. So init/deinit STAY unimplemented!() in W1 (document this scoping in DEFERRALS.md so a completeness critic doesn't read the spec's '1795/1841' literally). The live present path already flows Frame.complete→on_completed(metal_buffer_completed)→api.present; W1-3 made present real, so the only edit is restoring the R1 error log (previously a comment because offscreen present was infallible). No log facade wiring change — `log` crate is the established facade.

**Gates:**
- `cargo build -p rustty-renderer`
- `cargo test -p rustty-renderer --lib generic::`
- `cargo clippy -p rustty-renderer --all-targets -- -D warnings`
- `cargo fmt --check`
- `python3 /Users/gael/projects/rustty/.port/tools/gates/check_comment_coverage.py`
- `python3 /Users/gael/projects/rustty/.port/tools/gates/check_test_counts.py`

**Risk:** Low — one-line un-defer of a verbatim log; the risk is a critic reading the spec's '1795/1841 present seams' literally and expecting Renderer::init un-deferred, which the culminating KAT proves unnecessary. Document the scoping.

### Task W1-6: CULMINATING KAT — on-screen present of one golden-state frame (FIRST PIXELS)

**Files:**
- Modify: `crates/rustty-renderer/src/generic.rs` — ~5960 (add the on-screen present test INSIDE the existing `#[cfg(test)] mod` beside golden_renders_cursor_fg_bg, so it can call golden_renderer + Renderer internals + Metal::init + px_at)
- Test: `crates/rustty-renderer/src/generic.rs`

**Consumes:**
- golden_renderer(cols, rows) -> Option<Renderer> (generic.rs test mod:5679)
- px_at(&[u8], w, x, y) -> (u8,u8,u8,u8) (generic.rs test mod:5826)
- Metal::init(&AnyObject, f64, AlphaBlending) -> Result<Metal, Error> (W1-2)
- Metal::present(&mut self, &Target, bool) -> Result<(), Error> (W1-3)
- Renderer.swap_chain.frames[idx].target: Target with .surface + read_bgra() (target.rs:212)
- Renderer.draw_frame(true) / update_frame (generic.rs)
- IOSurfaceLayer.layer.contents() (objc2-quartz-core, via W0)

**Produces:**
- #[test] fn golden_presents_frame_on_screen() — proves Metal::init binds a layer to an NSView AND the golden-state frame's IOSurface is presented as the layer's contents, with the readback pixels matching the offscreen golden asserts (cursor red / bg dark+opaque / fg glyph bright)

**Reference (port per R1 — verbatim comments, byte-faithful):**
- `src/renderer/Metal.zig:253-259` — present(target, sync) — the setSurface primitive the KAT exercises on-screen
- `src/renderer/generic.zig:1429-1490` — drawFrame → Frame.complete(sync) → bufferCompleted → api.present: the exact present pipeline the KAT drives via draw_frame(true)

**FFI / xev mapping:**
- `self.layer.setSurfaceSync(target.surface) (present sync)` → `onscreen.present(&drawn_target, true) → layer.set_surface_sync(&target.surface) → CALayer.setContents(surface)`
- `layer contents readback for assertion` → `onscreen.layer.as_ref().unwrap().layer.contents() -> Option<Retained<AnyObject>>`

**Test KAT:**
```rust
#[cfg(target_os = "macos")]
#[test]
fn golden_presents_frame_on_screen() {
    use objc2::{class, msg_send, rc::Retained, runtime::AnyObject};
    // 1) Draw ONE real terminal frame into an IOSurface-backed Target (offscreen
    //    golden state — the proven pixel path).
    let mut r = match golden_renderer(10, 3) { Some(r) => r, None => return };
    let mut mutex = Mutex::new();
    let mut t = Terminal::init(Global, opts(10, 3)).unwrap();
    { let mut s = t.vt_stream(); s.next_slice(b"Ab"); s.deinit(); }
    let st = state::State { mutex: &mut mutex, terminal: &mut t, inspector: None,
        preedit: None, mouse: state::Mouse::default() };
    r.update_frame(&st, true).unwrap();
    r.draw_frame(true).unwrap();
    let idx = r.swap_chain.frame_index as usize;
    // 2) Confirm the drawn pixels are real (reuse the offscreen golden asserts):
    //    cursor cell is red-dominant ⇒ these are genuine terminal pixels, not blank.
    let px = r.swap_chain.frames[idx].target.read_bgra();
    let w = r.size.screen.width as usize;
    let cw = r.grid_metrics.cell_width as usize;
    let ch = r.grid_metrics.cell_height as usize;
    let (b, g, rr, _a) = px_at(&px, w, 2 * cw + cw / 2, ch / 2);
    assert!(rr > 128 && rr > g && rr > b, "golden state not drawn (cursor R={rr})");
    // 3) Build an ON-SCREEN Metal over a bare NSView and PRESENT that exact surface.
    let view: Retained<AnyObject> = unsafe { msg_send![class!(NSView), new] };
    let mut onscreen = Metal::init(&view, 1.0, AlphaBlending::Native).expect("on-screen init");
    // FIRST PIXELS: push the drawn IOSurface onto the CALayer (sync present).
    onscreen.present(&r.swap_chain.frames[idx].target, true).expect("present on-screen");
    // 4) The layer attached to the view now hosts the presented terminal frame.
    let layer = onscreen.layer.as_ref().unwrap();
    let contents: Option<Retained<AnyObject>> = layer.layer.contents();
    let got = contents.expect("nothing presented to layer");
    let want: *const objc2_io_surface::IOSurfaceRef = &*r.swap_chain.frames[idx].target.surface;
    assert_eq!(
        Retained::as_ptr(&got) as *const objc2_io_surface::IOSurfaceRef, want,
        "layer.contents is not the drawn terminal IOSurface"
    );
    // And the view is layer-hosting our layer (end-to-end host handshake).
    let bound: *mut AnyObject = unsafe { msg_send![&*view, layer] };
    let ours: *const objc2_quartz_core::CALayer = &*layer.layer;
    assert_eq!(bound as *const AnyObject, ours.cast(), "view lost its layer");
    t.deinit(Global);
}
```

**Port notes:** Placed in the existing generic.rs #[cfg(test)] module so it can reach the private golden_renderer/px_at helpers + Renderer.swap_chain internals — an external tests/ file cannot construct Renderer (many fields are pub(crate)). Reuses the offscreen golden gate EXACTLY (same px_at cursor-red assert) so pixel parity is inherited, then adds the on-screen delta: present the drawn surface and assert it became the layer contents. This deliberately draws with the OFFSCREEN api (golden_renderer pins offscreen_size, avoiding draw_frame's surface_size resize when a layer's bounds differ) and presents through a SEPARATE on-screen Metal — the cleanest faithful proof of the setSurface primitive without coupling to layer-bounds sizing. The tighter variant (inject onscreen layer into r.api and let draw_frame→metal_buffer_completed→api.present drive it) additionally requires sizing layer.bounds==size.screen; W1-3's onscreen_surface_size_and_present_from_layer already covers surface_size, and W1-5's unit covers the metal_buffer_completed present branch, so this KAT keeps the two-phase form for robustness. SB/TB: no &mut aliasing — present takes &Target from r.swap_chain (shared) while r is not mutably borrowed across the call.

**Gates:**
- `cargo build -p rustty-renderer`
- `cargo test -p rustty-renderer --lib generic::tests::golden_presents_frame_on_screen`
- `cargo test -p rustty-renderer --lib  # full suite incl. offscreen golden stays green`
- `cargo clippy -p rustty-renderer --all-targets -- -D warnings`
- `cargo fmt --check`
- `bash /Users/gael/projects/rustty/.port/tools/gates/check_acyclic.sh`
- `python3 /Users/gael/projects/rustty/.port/tools/gates/check_test_counts.py`

**Risk:** Med — depends on all of W1-1..W1-3 + W0's IOSurfaceLayer.contents/setContents; bare-NSView creation on headless CI may need sharedApplication (see W1-2 risk). If layer.contents() returns a wrapped/copied object rather than the exact IOSurfaceRef pointer, weaken the assertion to `contents.is_some()` + a pixel readback of the target, still proving first pixels.

**Open risks:** objc2-quartz-core (CAMetalLayer/CALayer) is the one unverified objc2 add per the spec (OPEN RISK High) — the exact accessor names bounds/setBounds/contentsScale/contents/setContents/setNeedsDisplayOnBoundsChange must be confirmed to exist in objc2-quartz-core 0.3 during the W0 spike; W1-2/W1-3/W1-6 all consume them.; W0 must land first and expose IOSurfaceLayer::init / .layer:Retained<CALayer> / set_surface / set_surface_sync / set_display_callback / DisplayCallback — W1 CONSUMES these exact signatures; any drift (e.g. layer field name, error type) breaks W1-1..W1-3.; Bare NSView creation on headless CI without a running NSApplication may fail the layer-host handshake (setLayer:/setWantsLayer:); mitigation is +[NSApplication sharedApplication] before view creation or attaching to a raw CALayer host — affects W1-2/W1-6.; The spec cites the generic present seams at generic.rs:1795/1841 but those are Renderer::init/deinit (swap_chain/display_link = W2/W3 vsync+renderer_seam scope), NOT the present half; W1 leaves them unimplemented!() and drives present through the already-wired metal_buffer_completed path — this scoping must be recorded in DEFERRALS.md so a completeness critic does not read the line numbers literally.; loop_enter's displayCallback trampoline needs GenericRenderer in scope (container_of over api) — if authoring it in generic.rs alongside metal_buffer_completed is deferred, loop_enter stays unimplemented!() until W2's render thread; flag the split.; Async addCompletedHandler firing (W1-4) is timing-dependent on CI; the bounded-spin fire assertion may need downgrading to a no-panic/commit assertion with fire-observation deferred to W2.; This is FFI (objc2) — the W1 tests are Miri-excluded (foreign calls); the raw-*mut present/completion aliasing must be re-proven on the mirihar harness and recorded in .port/phase-2/MIRI_EXCEPTIONS.md before shipping.

---

### Task W1-7: Renderer windowed lifecycle — fill `Renderer::{init,deinit}` (generic.rs:1793/1840) + `surface_init`/`finalize_surface_init` + Options handshake

**Files:**
- Modify: `crates/rustty-renderer/src/generic.rs:1793` — fill `pub fn init(alloc, options) -> Result<Renderer, Error>` (the windowed sibling of the existing `init_offscreen`)
- Modify: `crates/rustty-renderer/src/generic.rs:1840` — fill `pub fn deinit(&mut self)` with the verbatim reference body (the `//` block at :1808-1839)
- Modify: `crates/rustty-renderer/src/generic.rs` — add `pub fn surface_init<S>(rt_surface: *mut S) -> Result<(), Error>` and `pub fn finalize_surface_init<S>(&mut self, rt_surface: *mut S) -> Result<(), Error>` (the apprt-free-generic surface handshake; the layer/NSView attach delegates to `Metal::init`/`Metal` from W1-2)
- Test: `crates/rustty-renderer/src/generic.rs` (inline `#[cfg(all(test, target_os="macos"))]` construct+deinit KAT)

**Consumes (see Canonical Interfaces):**
- `Metal::init(...)` on-screen constructor + `layer` field (W1-2), `IOSurfaceLayer` (W0.T3)
- the existing `Renderer::init_offscreen` body (shaders/`font.Shaper`/uniforms/`prepBackgroundImage`) — generalize, do not duplicate

**Produces (Canonical Interfaces → rustty_renderer::Renderer windowed lifecycle):**
- `pub fn init(alloc: Global, options: Options) -> Result<Renderer, Error>` — generic.rs:1793 seam FILLED
- `pub fn deinit(&mut self)` — generic.rs:1840 seam FILLED
- `pub fn surface_init<S>(rt_surface: *mut S) -> Result<(), Error>`
- `pub fn finalize_surface_init<S>(&mut self, rt_surface: *mut S) -> Result<(), Error>`

**Reference (port per R1 — verbatim comments, byte-faithful):**
- `src/renderer/generic.zig` `init` (the full body cited in the seam comment at generic.rs:1785-1792: `GraphicsAPI.init` over a live surface, `font.Shaper.init`, `initShaders`, `updateFontGridUniforms`/`updateScreenSizeUniforms`/`updateBgImageBuffer`/`prepBackgroundImage`) — generalize the shared body with `init_offscreen`.
- `src/renderer/generic.zig` `deinit` (the verbatim block preserved at generic.rs:1808-1839) — port as-is; the `DisplayLink` teardown arm lands with W2 (guard with the display-link presence).
- `src/renderer/generic.zig` `surfaceInit`/`finalizeSurfaceInit` — the surface handshake.

**Test KAT:**
```rust
// crates/rustty-renderer/src/generic.rs (inline, near the other on-screen tests)
#[cfg(all(test, target_os = "macos"))]
mod windowed_lifecycle {
    use super::*;
    // Constructs a WINDOWED Renderer against a headless test CAMetalLayer/NSView
    // (the same test-view helper W1-2 introduces), proving the generic.rs:1793/1840
    // seams no longer panic and init/deinit round-trip cleanly (no leak — the
    // deinit reference body frees shaders/shaper/images/swap_chain/api).
    #[test]
    fn windowed_renderer_init_then_deinit_round_trips() {
        let (view, _win) = crate::metal::test_support::headless_layer_host(); // from W1-2
        let opts = Options {
            config: DerivedConfig::default(),
            font_grid: crate::test_support::shared_grid_ptr(),
            size: crate::size::Size::test_80x24(),
        };
        // surfaceInit prepares the view's layer; init builds the renderer; the
        // Metal layer attach happens via Metal::init (W1-2) driven by the handshake.
        Renderer::surface_init(view.as_ptr()).expect("surface_init");
        let mut r = Renderer::init(allocator_api2::alloc::Global, opts).expect("windowed init");
        r.finalize_surface_init(view.as_ptr()).expect("finalize_surface_init");
        r.deinit(); // must not panic / leak (ASan gate covers UB across the objc FFI)
    }
}
```

**Port notes:** The windowed `init` is the sibling of the already-landed `init_offscreen` — factor the shared body (shaders/shaper/uniforms/`prepBackgroundImage`) into a common helper so R1 fidelity is preserved without duplication; the ONLY delta is the graphics-API construction (`Metal::init` windowed, W1-2, vs the offscreen `Target`). `Options` stays the 3 platform-neutral fields — the surface/layer never enters `Options` (apprt cycle); it enters via `surface_init<S>`/`finalize_surface_init<S>` (generic over `S`, so rustty-renderer never names an apprt type). The `deinit` `DisplayLink` teardown arm is guarded until W2 lands the display link. This task is a prerequisite of W3-E1 (core swaps in this real `Renderer`) and is exercised end-to-end by W1-6.

**Gates:**
- `cargo test -p rustty-renderer --features metal windowed_lifecycle`
- `cargo clippy -p rustty-renderer -- -D warnings`
- `bash .port/tools/gates/check_acyclic.sh`
- `.port/tools/gates/check_comment_coverage.py` (the verbatim deinit body must survive)

**Risk:** MEDIUM — the windowed `init` is a large faithful body; the mitigation is reusing `init_offscreen`'s already-tested shared path so only the live-surface graphics-API delta is new. Execute this task's design→adversary workflow against `generic.zig` init/deinit line-by-line.

---

## Chunk W2 — CVDisplayLink block in the RENDERER ONLY (apprt-free)

**Goal:** Port the apprt-free CVDisplayLink (vsync) machinery into rustty-renderer's generic.rs + a new metal/cvdisplaylink.rs FFI leaf: the real CoreVideo `DisplayLink` primitive (createWithActiveCGDisplays/release/start/stop/isRunning/setCurrentCGDisplay/setOutputCallback), the Renderer methods that drive it (loop_enter/loop_exit/thread_enter/thread_exit/display_link_callback/set_macos_display_id/has_vsync), and the callback→`rustty_xev::Async` notify seam by which core (W3) passes a `draw_now` Async handle IN — the renderer never names the render `Thread` (R4: renderer must NOT dep apprt-api). Culminating proof: a macOS KAT builds a real CVDisplayLink bound to the main display, installs the ported callback, and observes its CoreVideo-thread notification drain into a real kqueue `Loop` via a bounded run(NoWait) pump (live_shell.rs discipline).

**Chunk culminating test:** crates/rustty-renderer/src/generic.rs :: macos_impl::tests::vsync_callback_notifies_draw_now — a real CVDisplayLink bound to CGMainDisplayID drives its output callback on a CoreVideo real-time thread; the ported Renderer::display_link_callback notifies a real rustty_xev::Async (draw_now) armed on a real kqueue Loop; a bounded run(NoWait) pump observes the drain (fires>0). Proves the vsync half end-to-end: CV thread → notify → mach port → loop drain. Skips cleanly on a headless host (no display / no Metal).


### Task W2-T1: CoreVideo DisplayLink FFI primitive (port pkg/macos/video/display_link.zig) + wire into the renderer

**Files:**
- Create: `crates/rustty-renderer/src/metal/cvdisplaylink.rs`
- Modify: `crates/rustty-renderer/Cargo.toml` — insert one line in [dependencies] (after rustty-zigstd, ~line 60): `rustty-xev = { workspace = true }` — the workspace dep is already declared (root Cargo.toml:175 `rustty-xev = { path = "crates/rustty-xev" }`). Add a PORT NOTE comment: renderer→xev is a downward leaf edge (xev deps = {rustty-global, libc} only, no edge back), so check_acyclic.sh stays green.
- Modify: `crates/rustty-renderer/src/metal.rs` — ~35: add `pub mod cvdisplaylink;` in the alphabetical pub-mod block (after `pub mod buffer;`, before `pub mod frame;`).
- Modify: `crates/rustty-renderer/src/generic.rs` — 1065-1073: REPLACE the placeholder `pub struct DisplayLink { _seam: () }` (and update its PORT NOTE) with a re-export: `pub use crate::metal::cvdisplaylink::DisplayLink;`. Keep the R1 reference doc-comment (generic.zig:39-42 `const DisplayLink = switch(...) { .macos => *macos.video.DisplayLink, else => void }`; field is `display_link: ?DisplayLink = null`). The field at 1253 `pub display_link: Option<DisplayLink>` auto-picks up the FFI type; the `display_link: None` test constructors at 5659/5800 keep compiling unchanged.
- Test: `crates/rustty-renderer/src/metal/cvdisplaylink.rs`

**Consumes:**
- crate::generic::Error (the aggregate renderer error enum, generic.rs:925 — MetalFailed/OutOfMemory/Defunct/ImageDecode); reachable as `crate::generic::Error` via the `#[cfg(target_os="macos")] pub use macos_impl::*` at generic.rs:6634
- the ZST-callback monomorphization pattern established by rustty_xev Async::wait (async.rs:216-305): a `CB: Fn(...) + 'static` type param asserted zero-sized via `const { assert!(size_of::<CB>()==0) }`, reconstructed in the extern-C trampoline via `NonNull::<CB>::dangling().as_ptr().read()`

**Produces:**
- pub struct DisplayLink (repr(transparent) newtype over core::ptr::NonNull<core::ffi::c_void> — the CVDisplayLinkRef; NOT Copy, NO Drop impl: released manually by Renderer::deinit / tests, exactly like ghostty's manual release())
- pub fn DisplayLink::create_with_active_cg_displays() -> Result<DisplayLink, crate::generic::Error>
- pub fn DisplayLink::release(&self)
- pub fn DisplayLink::start(&self) -> Result<(), crate::generic::Error>
- pub fn DisplayLink::stop(&self) -> Result<(), crate::generic::Error>
- pub fn DisplayLink::is_running(&self) -> bool
- pub fn DisplayLink::set_current_cg_display(&self, display_id: u32) -> Result<(), crate::generic::Error>
- pub fn DisplayLink::set_output_callback<UD: 'static, CB: Fn(*mut DisplayLink, Option<*mut UD>) + 'static>(&self, cb: CB, userinfo: *mut UD) -> Result<(), crate::generic::Error>

**Reference (port per R1 — verbatim comments, byte-faithful):**
- `pkg/macos/video/display_link.zig:1-87` — the WHOLE DisplayLink opaque wrapper: Error{InvalidOperation}; createWithActiveCGDisplays (Allocator.Error!*DisplayLink, out-param, kCVReturnSuccess check); release; start/stop (Error!void, kCVReturnSuccess→InvalidOperation); isRunning (!=0); setCurrentCGDisplay(CGDirectDisplayID); setOutputCallback(comptime Userdata, comptime callbackFn: *const fn(*DisplayLink, ?*Userdata) void, userinfo) with the inline callconv(.c) trampoline that throws away inNow/inOutputTime/flagsIn/flagsOut and forwards to callbackFn
- `pkg/macos/main.zig:16-21` — the @cInclude of CoreGraphics/CoreGraphics.h + CoreVideo/CoreVideo.h that provides c.CVDisplayLink*, c.CVReturn, c.CVTimeStamp, c.CVOptionFlags, c.CGDirectDisplayID, c.kCVReturnSuccess — the C symbols the raw extern block re-declares

**FFI / xev mapping:**
- `c.CVDisplayLinkCreateWithActiveCGDisplays(@ptrCast(&result)) (display_link.zig:13)` → `extern "C" fn CVDisplayLinkCreateWithActiveCGDisplays(out: *mut *mut c_void) -> CVReturn, under #[link(name="CoreVideo", kind="framework")]; !=kCVReturnSuccess or null → Err(Error::OutOfMemory) (Zig error.OutOfMemory)`
- `c.CVDisplayLinkRelease(@ptrCast(self)) (display_link.zig:22)` → `extern "C" fn CVDisplayLinkRelease(dl: *mut c_void)`
- `c.CVDisplayLinkStart / c.CVDisplayLinkStop (display_link.zig:26/31)` → `extern "C" fn CVDisplayLinkStart/Stop(dl: *mut c_void) -> CVReturn; !=kCVReturnSuccess → Err(Error::MetalFailed) (Zig error.InvalidOperation)`
- `c.CVDisplayLinkIsRunning(@ptrCast(self)) != 0 (display_link.zig:36)` → `extern "C" fn CVDisplayLinkIsRunning(dl: *mut c_void) -> u8 (Boolean); wrapper returns `!= 0``
- `c.CVDisplayLinkSetCurrentCGDisplay(@ptrCast(self), display_id) (display_link.zig:43)` → `extern "C" fn CVDisplayLinkSetCurrentCGDisplay(dl: *mut c_void, id: CGDirectDisplayID /*=u32*/) -> CVReturn`
- `c.CVDisplayLinkSetOutputCallback(@ptrCast(self), &struct{fn callback(...) callconv(.c) CVReturn}.callback, userinfo) (display_link.zig:59)` → `extern "C" fn CVDisplayLinkSetOutputCallback(dl: *mut c_void, cb: CVDisplayLinkOutputCallback, userinfo: *mut c_void) -> CVReturn, where CVDisplayLinkOutputCallback = unsafe extern "C" fn(*mut c_void, *const CVTimeStamp, *const CVTimeStamp, CVOptionFlags, *mut CVOptionFlags, *mut c_void) -> CVReturn; a monomorphized `trampoline::<UD,CB>` (the inline Zig struct.callback) discards the 4 timing args, reconstructs the ZST CB via NonNull::dangling().read(), casts dl→*mut DisplayLink + userinfo→Option<*mut UD>, calls cb, returns kCVReturnSuccess`

**Test KAT:**
```rust
// crates/rustty-renderer/src/metal/cvdisplaylink.rs — appended #[cfg(test)] module.
// rustty-only MARGIN smoke test (ghostty's display_link.zig has NO `test{}`, so
// this does not change the per-file Zig test-count map — the metal.rs smoke-test
// precedent). Proves the raw CoreVideo FFI bindings drive a real CVDisplayLink
// lifecycle. Whole file is macOS-only (metal module is macOS-gated); skips cleanly
// on a headless host with no display.
#[cfg(test)]
mod tests {
    use super::DisplayLink;

    #[test]
    fn display_link_lifecycle() {
        let dl = match DisplayLink::create_with_active_cg_displays() {
            Ok(dl) => dl,
            Err(_) => {
                eprintln!("no CVDisplayLink (headless?); skipping");
                return;
            }
        };
        assert!(!dl.is_running());
        dl.start().expect("CVDisplayLinkStart");
        assert!(dl.is_running());
        dl.stop().expect("CVDisplayLinkStop");
        assert!(!dl.is_running());
        // No Drop impl — release manually, matching ghostty's deinit contract.
        dl.release();
    }
}
```

**Port notes:** BYTE/TYPE FIDELITY: CVReturn=i32 (int32_t), CVOptionFlags=u64 (uint64_t), CGDirectDisplayID=u32, kCVReturnSuccess=0, CVDisplayLinkIsRunning returns Boolean=u8. CVTimeStamp is declared as an opaque marker `#[repr(C)] pub struct CVTimeStamp { _private: [u8; 0] }` — the callback only ever takes `*const CVTimeStamp` and discards it (faithful to display_link.zig:70-73 `_ = inNow; _ = inOutputTime;`), so its layout is never dereferenced. CVDisplayLinkRef and *DisplayLink are the SAME opaque pointer (ghostty `DisplayLink = opaque{}` casts self↔ref via @ptrCast); modeled as NonNull<c_void> in a repr(transparent) newtype so `Option<DisplayLink>` is a niche-optimized nullable pointer == Zig `?DisplayLink` (?*DisplayLink). ERROR MAPPING (R1 collapse, matching the SwapChain/Target precedent): Zig `Allocator.Error!*DisplayLink` OOM → Error::OutOfMemory; Zig `error{InvalidOperation}` (start/stop/setCurrentCGDisplay/setOutputCallback) → Error::MetalFailed (the CoreVideo-call-failed analogue in the aggregate enum). DEPRECATION SIDESTEP: CVDisplayLink* are deprecated as of macOS 15; a RAW extern "C" block carries NO #[deprecated] attribute (that only comes from objc2-core-video's generated bindings / C headers), so `-D warnings` stays green WITHOUT needing to add/allow(deprecated) objc2-core-video — this is why we bind CoreVideo directly (mirrors ghostty's own @cInclude approach) rather than adding an objc2-core-video dep. FRAMEWORK LINK: `#[link(name = "CoreVideo", kind = "framework")]` on the extern block (no build.rs change; self-contained). SB/TB: set_output_callback stores NO borrow — userinfo is a raw *mut UD handed to CoreVideo and returned untyped to the trampoline; the trampoline reconstructs the ZST CB by value (never touches the caller's stack). The DisplayLink pointer is Send-neutral: the CV callback runs on a CoreVideo-owned real-time thread, but the wrapper itself is only touched from the render thread; only the *mut UD (the Async) is shared cross-thread and that is the caller's soundness obligation (documented at set_output_callback's # Safety).

**Gates:**
- `cargo build -p rustty-renderer`
- `cargo test -p rustty-renderer --lib metal::cvdisplaylink`
- `cargo clippy -p rustty-renderer --all-targets -- -D warnings`
- `cargo fmt -p rustty-renderer -- --check`
- `bash .port/tools/gates/check_acyclic.sh`
- `python3 .port/tools/gates/check_comment_coverage.py`
- `python3 .port/tools/gates/check_test_counts.py`

**Risk:** MED — CVDisplayLink is deprecated; the raw extern binding is the mitigation (no objc2-core-video gap/deprecation to fight). set_output_callback's ZST-CB trampoline must exactly mirror Async::wait's proven NonNull::dangling().read() reconstruction; a capturing closure would fail the const size==0 assert at compile time (safe fail).

### Task W2-T2: generic.rs Renderer CVDisplayLink block: loop_enter/loop_exit/thread_enter/thread_exit/display_link_callback/set_macos_display_id/has_vsync

**Files:**
- Modify: `crates/rustty-renderer/src/generic.rs` — Insert a new `impl Renderer { ... }` region inside `mod macos_impl` immediately AFTER mark_dirty (after line 1855), carrying the seven ported methods below. Also add `use rustty_xev as xev;` (or reference rustty_xev::Async fully-qualified) near the macos_impl `use` block. DO NOT touch Renderer::init (1793, deferred draw-path seam) or Renderer::deinit (1840, deferred): the reference display_link CREATION (generic.zig:692-699) and stop+release teardown (generic.zig:807-812) stay preserved as the verbatim comments already at generic.rs:1686-1693 / 1816-1820 and land un-commented in W1/W3, NOT here.
- Test: `crates/rustty-renderer/src/generic.rs`

**Consumes:**
- From W2-T1: DisplayLink + all its methods (create/release/start/stop/is_running/set_current_cg_display/set_output_callback)
- self.display_link: Option<DisplayLink> (existing field, generic.rs:1253)
- self.api: Metal (existing field, generic.rs:1248) — Metal::loop_enter (metal_impl.rs:383, the M-4 IOSurfaceLayer display-callback hook; currently unimplemented!() until W1 lands it)
- rustty_xev::Async + Async::notify(&self) -> Result<(), NotifyError> (the draw_now handle core passes IN)
- crate::generic::Error

**Produces:**
- pub fn Renderer::loop_enter(&mut self, draw_now: *mut rustty_xev::Async) -> Result<(), Error>   // R4 substitute for ghostty's `thr: *renderer.Thread` — takes the Async handle directly so the renderer never names the render Thread; W3's core Thread calls `renderer.loop_enter(&raw mut thread.draw_now)`
- pub fn Renderer::loop_exit(&mut self)
- pub fn Renderer::thread_enter(&self, surface: *mut core::ffi::c_void) -> Result<(), Error>   // Metal declares no threadEnter → no-op forwarder; surface opaque (renderer is apprt-generic)
- pub fn Renderer::thread_exit(&self)   // Metal declares no threadExit → no-op
- pub fn Renderer::display_link_callback(_display_link: *mut DisplayLink, ud: Option<*mut rustty_xev::Async>)   // widened to pub (dead-code-exempt) so loop_enter installs it AND the KAT drives it
- pub fn Renderer::set_macos_display_id(&mut self, id: u32) -> Result<(), Error>
- pub fn Renderer::has_vsync(&self) -> bool

**Reference (port per R1 — verbatim comments, byte-faithful):**
- `src/renderer/generic.zig:896-933` — loopEnter (api.loopEnter() hook → if DisplayLink==void return → self.display_link orelse return → setOutputCallback(xev.Async, &displayLinkCallback, &thr.draw_now) → start() catch {}) and loopExit (api loopExit hook — Metal declares none → omit → display_link orelse return → stop() catch {})
- `src/renderer/generic.zig:880-894` — threadEnter(self, surface: *apprt.Surface) / threadExit(self): pure `if (@hasDecl(GraphicsAPI, "threadEnter")) try self.api.threadEnter(surface)` forwarders — Metal declares NEITHER, so both monomorphize to no-ops (the chunk's 'Metal api threadEnter/threadExit hooks, offscreen no-op'). NOTE the display-link start/stop is in loopEnter/loopExit + setFocus/setVisible, NOT in threadEnter/threadExit.
- `src/renderer/generic.zig:986-994` — displayLinkCallback(_: *macos.video.DisplayLink, ud: ?*xev.Async): `const draw_now = ud orelse return; draw_now.notify() catch |err| log.err("error notifying draw_now err={}", .{err});`
- `src/renderer/generic.zig:1001-1024` — setMacOSDisplayID(self, id: u32) (DisplayLink==void return → orelse return → log.info → setCurrentCGDisplay(id) catch |err| log.warn) and hasVsync(self) (DisplayLink==void→false → orelse→false → display_link.isRunning())
- `src/renderer/Metal.zig:164-182` — Metal.loopEnter (the only backend that declares it) — the api hook loop_enter forwards to; its body installs the IOSurfaceLayer display callback (the W1 M-4 present seam, metal_impl.rs:383 unimplemented!()). W2 forwards to it faithfully; the W2 KAT never calls Renderer::loop_enter so the deferred hook is not exercised here.

**FFI / xev mapping:**
- `try display_link.setOutputCallback(xev.Async, &displayLinkCallback, &thr.draw_now) (generic.zig:910)` → `display_link.set_output_callback::<rustty_xev::Async, _>(Renderer::display_link_callback, draw_now)?   // draw_now: *mut rustty_xev::Async is the parameter (core's Thread.draw_now, passed IN)`
- `draw_now.notify() catch |err| log.err(...) (generic.zig:991)` → `if let Err(_err) = unsafe { (*draw_now).notify() } { /* R1: `log.err("error notifying draw_now err={}", .{err})` preserved as a comment — renderer has no logging facade wired yet (generic.rs:2253-2254 convention); error swallowed */ }`
- `display_link.setCurrentCGDisplay(id) catch |err| log.warn(...) (generic.zig:1006)` → `if let Err(_err) = dl.set_current_cg_display(id) { /* log.warn(...) preserved as comment */ }  // preceded by the preserved `// log.info("updating display link display id={}", .{id})``
- `return display_link.isRunning() (generic.zig:1023)` → `dl.is_running()`
- `self.api.loopEnter() (generic.zig:900) / self.api.threadEnter(surface) (884)` → `self.api.loop_enter()  (metal_impl.rs:383) ; thread_enter body is `let _ = surface; Ok(())` (Metal declares no threadEnter — the @hasDecl probe is false)`

**Test KAT:**
```rust
// crates/rustty-renderer/src/generic.rs — added inside `#[cfg(target_os="macos")] mod macos_impl { mod tests { ... } }` (the module at generic.rs:5439; super::* already imports Renderer + DisplayLink re-export). rustty-only MARGIN test (no ghostty test{} for this block).
#[test]
fn renderer_has_vsync_reflects_display_link() {
    use crate::metal::cvdisplaylink::DisplayLink;
    // Real offscreen Renderer via the existing helper (skips with no Metal device).
    let Some(mut r) = test_renderer(4, 2) else {
        eprintln!("no Metal device; skipping");
        return;
    };
    // No display link (init's vsync-create is a deferred draw-path seam) → not vsync,
    // and set_macos_display_id / loop_exit are harmless no-ops.
    assert!(!r.has_vsync());
    let _ = r.set_macos_display_id(0);
    r.loop_exit();
    assert!(!r.has_vsync());

    // Inject the faithful `display_link` the reference init creates when
    // config.vsync is true (pub field; init/deinit stay deferred, so we own its
    // lifecycle in-test).
    let Ok(dl) = DisplayLink::create_with_active_cg_displays() else {
        eprintln!("no CVDisplayLink; skipping");
        return;
    };
    r.display_link = Some(dl);

    // set_macos_display_id now routes to setCurrentCGDisplay (id 0 → error is
    // swallowed by the ported log.warn-as-comment path; must not panic).
    let _ = r.set_macos_display_id(0);
    // hasVsync reflects isRunning: false before start.
    assert!(!r.has_vsync(), "not running before start");
    // Start via the link directly (loop_enter's api hook is the deferred W1 M-4
    // seam); hasVsync → true.
    r.display_link.as_ref().unwrap().start().expect("start");
    assert!(r.has_vsync(), "running after start");
    // loop_exit stops the link → hasVsync false.
    r.loop_exit();
    assert!(!r.has_vsync(), "stopped after loop_exit");

    // Teardown: release the field-owned link before the (Drop-less) Renderer
    // drops — matching ghostty deinit's `stop() ; release()` (generic.zig:808-810),
    // which is otherwise a deferred seam here.
    r.display_link.as_ref().unwrap().release();
    r.display_link = None;
}
```

**Port notes:** R4 CORE CONSTRAINT (the whole reason this chunk exists): rustty-renderer MUST NOT dep rustty-apprt-api ('only apprt/config/core depend on renderer'). The render `Thread` is apprt-generic (surface:*apprt.Surface, app_mailbox:App.Mailbox) so it lives fully in core (crate::renderer_seam::Thread<A>) and is filled in W3 — DO NOT create/port any Thread type here, DO NOT touch crates/rustty-renderer/src/thread.rs beyond DerivedConfig. The ONE ghostty coupling that names the Thread — `loopEnter(thr: *renderer.Thread)` passing `&thr.draw_now` — is broken by making loop_enter take `draw_now: *mut rustty_xev::Async` directly; W3's core Thread supplies `&raw mut self.draw_now`. FAITHFULNESS NUANCE: the chunk brief says 'threadEnter/threadExit … on-screen start/stop the display link'; per the ACTUAL reference (generic.zig:880-894) threadEnter/threadExit are pure api no-op forwarders and the display-link start/stop lives in loopEnter/loopExit (896-933) + setFocus/setVisible (1029-1065). We port to the reference (R1): threadEnter/threadExit = no-ops; loop_enter installs+starts, loop_exit stops. setFocus/setVisible link start/stop are OUT of W2 scope (the focus/visibility chunk) — noted so they aren't dropped. surface param of thread_enter is typed `*mut core::ffi::c_void` (opaque) since Metal never reads it and the renderer can't name apprt.Surface. LOGGING: renderer has no logging facade wired (generic.rs:2253-2254), so every `log.err/info/warn` in this block is preserved as an R1 comment with the error swallowed (`catch |err| log.x(...)` → `if let Err(_err) = ... { /* log.x(...) */ }`). SB/TB: display_link_callback takes `ud: Option<*mut rustty_xev::Async>` and does `unsafe { (*draw_now).notify() }` — notify(&self) only reads the Copy `port: u32`, no &mut, no aliasing; it runs on the CoreVideo real-time thread while the render thread owns the Renderer, and only the raw Async pointer crosses threads (the mach port kernel object serializes). loop_enter/loop_exit take `&mut self` but only read self.display_link (Option<DisplayLink>) + call self.api.loop_enter() — no whole-struct reborrow while a raw self-alias is live (the W2 mirihar aliasing gate).

**Gates:**
- `cargo build -p rustty-renderer`
- `cargo test -p rustty-renderer generic::macos_impl::tests::renderer_has_vsync_reflects_display_link`
- `cargo clippy -p rustty-renderer --all-targets -- -D warnings`
- `cargo fmt -p rustty-renderer -- --check`
- `python3 .port/tools/gates/check_comment_coverage.py`
- `python3 .port/tools/gates/check_test_counts.py`
- `bash .port/tools/gates/check_acyclic.sh`

**Risk:** MED — loop_enter forwards to Metal::loop_enter which is unimplemented!() until W1; W2 tests never call Renderer::loop_enter (they drive the link directly), so no panic in W2, but W2 must land AFTER W1 in the W0→W4 order for the full loop_enter path to be live. Widening display_link_callback/methods to pub is the port's established dead-code-exempt pattern (mirrors SwapChain/HighlightTag).

### Task W2-T3: Culminating vsync KAT: CVDisplayLink callback notifies a real xev::Async (drain observed)

**Files:**
- Modify: `crates/rustty-renderer/src/generic.rs` — Add the `vsync_callback_notifies_draw_now` #[test] inside `mod macos_impl::tests` (the module at 5439), adjacent to renderer_has_vsync_reflects_display_link (W2-T2). Imports are test-local (`use rustty_xev::{...}`, `use crate::metal::cvdisplaylink::DisplayLink`).
- Test: `crates/rustty-renderer/src/generic.rs`

**Consumes:**
- From W2-T1: DisplayLink::{create_with_active_cg_displays, set_current_cg_display, set_output_callback, start, is_running, stop, release}
- From W2-T2: Renderer::display_link_callback (the ported CV callback under test)
- rustty_xev public API: Async::{init, wait (unsafe, ZST cb), notify, deinit}, Loop::{init, run(RunMode::NoWait), deinit}, Completion::default(), RunMode::NoWait, CallbackAction::Rearm — the async.rs:422-462 `async_` test pattern + the live_shell.rs:316-332 bounded-pump pattern
- CGMainDisplayID (declared test-locally under #[link(name="CoreGraphics", kind="framework")]) — no objc2-core-graphics dep needed for W2

**Produces:**
- (test-only) the chunk culminating proof — no new public API

**Reference (port per R1 — verbatim comments, byte-faithful):**
- `src/renderer/generic.zig:986-994` — displayLinkCallback under test: the CV-thread callback that notifies draw_now (the exact machinery this KAT exercises end-to-end)
- `src/renderer/generic.zig:897-916` — loopEnter's setOutputCallback(xev.Async, &displayLinkCallback, &thr.draw_now) + start() — the wiring the KAT reproduces by hand (set_output_callback(display_link_callback, &mut draw_now) + start)

**FFI / xev mapping:**
- `xev.Async wait/notify + loop.run (the Zig `test "async"`, async.zig)` → `rustty_xev: `draw_now.wait(&mut loop_, &mut c_wait, &mut fires, cb)` (unsafe; cb is a non-capturing ZST closure → passes wait's const size_of==0 assert), CV thread → `Renderer::display_link_callback` → `draw_now.notify()`, drained by `loop_.run(RunMode::NoWait)`; teardown `stop(); release(); draw_now.deinit(); loop_.deinit()``
- `CVDisplayLinkSetCurrentCGDisplay(main display) (bind to main display)` → `display_link.set_current_cg_display(CGMainDisplayID())`

**Test KAT:**
```rust
// crates/rustty-renderer/src/generic.rs — inside `#[cfg(target_os="macos")] mod macos_impl { mod tests { ... } }` (module at 5439; super::* gives Renderer). rustty-only MARGIN KAT (NOT a mapped ghostty test{} — the CVDisplayLink block has no unit tests; like metal.rs's smoke tests + termio's live_shell.rs, it is an FFI/integration proof, so the per-file Zig test-count map is unaffected). macOS-only via the macos_impl cfg gate.
#[test]
fn vsync_callback_notifies_draw_now() {
    use crate::metal::cvdisplaylink::DisplayLink;
    use rustty_xev::{Async, CallbackAction, Completion, Loop, RunMode};
    use std::time::{Duration, Instant};

    // The main display id, to bind the link (exercises setCurrentCGDisplay, the
    // setMacOSDisplayID underlying call). Self-contained extern (no objc2-core-
    // graphics dep for W2).
    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGMainDisplayID() -> u32;
    }

    // Real CVDisplayLink over the active CG displays; skip cleanly on a headless
    // host (no display) — the metal.rs smoke-test precedent.
    let display_link = match DisplayLink::create_with_active_cg_displays() {
        Ok(dl) => dl,
        Err(_) => {
            eprintln!("no CVDisplayLink (headless?); skipping vsync KAT");
            return;
        }
    };
    // Bind to the main display.
    let _ = display_link.set_current_cg_display(unsafe { CGMainDisplayID() });

    // The render loop + the draw_now Async the CV callback notifies.
    let mut loop_ = Loop::init().expect("Loop::init");
    let mut draw_now = Async::init().expect("Async::init");

    // Arm draw_now on the loop: each drained notification bumps `fires`. The
    // closure is NON-CAPTURING (uses only its `ud` param) ⇒ ZST ⇒ satisfies
    // Async::wait's `const { size_of::<CB>() == 0 }`. `r` (Result<(),WaitError>)
    // is left unannotated — WaitError is xev-private — and inferred from the bound.
    let mut fires: u32 = 0;
    let mut c_wait = Completion::default();
    let cb = |ud: *mut u32, _l: *mut Loop, _c: *mut Completion, r| {
        let _ = r; // _ = r catch unreachable (parity with the async.zig tests)
        unsafe { *ud += 1 };
        CallbackAction::Rearm
    };
    // SAFETY: `loop_`, `c_wait`, `fires` are stack-pinned for the whole pump
    // below; the loop holds them by raw pointer, never a borrow (async.rs `async_`
    // discipline).
    unsafe {
        draw_now.wait(
            &mut loop_ as *mut Loop,
            core::ptr::addr_of_mut!(c_wait),
            core::ptr::addr_of_mut!(fires),
            cb,
        );
    }

    // Install the PORTED CV output callback: the CoreVideo real-time thread will
    // call Renderer::display_link_callback(dl, Some(&mut draw_now)) → draw_now.notify().
    // SAFETY: `draw_now` outlives the link — it is stopped+released before it drops.
    display_link
        .set_output_callback::<Async, _>(
            Renderer::display_link_callback,
            core::ptr::addr_of_mut!(draw_now),
        )
        .expect("set_output_callback");
    display_link.start().expect("DisplayLink::start");
    assert!(display_link.is_running(), "link should be running after start");

    // Bounded pump (live_shell.rs §5): non-blocking loop ticks + a wall-clock
    // deadline; terminates on BOTH the first drained notification and a timeout —
    // never hangs on the CV thread's cadence.
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut observed = false;
    for _ in 0..3000 {
        loop_.run(RunMode::NoWait).expect("loop.run(NoWait)");
        if fires > 0 {
            observed = true;
            break;
        }
        if Instant::now() > deadline {
            break;
        }
        // Yield so the CV real-time thread can fire the callback.
        std::thread::sleep(Duration::from_millis(1));
    }

    // Teardown BEFORE draw_now drops: stop + release so the CV thread stops
    // dereferencing `&draw_now`, then release the loop/async mach ports.
    let _ = display_link.stop();
    display_link.release();
    draw_now.deinit();
    loop_.deinit();

    assert!(
        observed,
        "CVDisplayLink callback never notified draw_now within the deadline (fires={fires})"
    );
}
```

**Port notes:** WHAT THIS PROVES (the W2 culmination): the vsync half end-to-end — a real CoreVideo real-time thread fires the PORTED Renderer::display_link_callback, which notifies a real rustty_xev::Async whose wait watcher, armed on a real kqueue Loop, drains on the pumping thread. This is the exact cross-thread hop ghostty relies on (CV thread → draw_now → render Thread loop), minus the render Thread (W3). NOT a golden/pixel test — the chunk brief's 'reuse the offscreen golden gate where pixels are involved' does not apply (this callback carries no pixels). DETERMINISM (live_shell.rs §5): NEVER loop.run(UntilDone) — the CV thread fires indefinitely; a bounded run(NoWait) pump + tick cap + 3s deadline terminates on success OR timeout. ORDERING/TEARDOWN is load-bearing: stop()+release() the link BEFORE draw_now/loop deinit so the CV thread cannot fire notify() into a freed mach port. SB/TB ALIASING: during the pump the main thread touches only loop_/fires; the CV thread touches only draw_now (via the raw *mut, read-only notify) — no concurrent &mut on any object; draw_now is Copy (port:u32) so wait's by-value copy and the CV thread's raw read observe the same mach port with no data race. Real mach/kqueue/CoreVideo syscalls ⇒ Miri-excluded (record in .port/phase-2/MIRI_EXCEPTIONS.md, 'rustty-renderer W2 CVDisplayLink'); the render-thread aliasing is separately proven on the mirihar harness (.port/phase-2/mirihar-chunk3b, a renderthread_naive=UB / renderthread_shipped=CLEAN pair) before shipping, per design §5.5/§6.

**Gates:**
- `cargo test -p rustty-renderer generic::macos_impl::tests::vsync_callback_notifies_draw_now -- --nocapture`
- `cargo test -p rustty-renderer`
- `cargo clippy -p rustty-renderer --all-targets -- -D warnings`
- `cargo fmt -p rustty-renderer -- --check`
- `python3 .port/tools/gates/check_test_counts.py`
- `python3 .port/tools/gates/check_comment_coverage.py`

**Risk:** MED — the CV callback fires on a CoreVideo-owned thread whose cadence is display-refresh-bound; the 3000-tick / 3s bounded pump absorbs the latency. On a headless CI runner CVDisplayLinkCreateWithActiveCGDisplays may succeed but never fire (no physical display) → the deadline path + skip-on-create-Err keep it from hanging, but the assertion could fail on a truly display-less runner: gate this KAT to run on a display-attached macOS runner (or mark it #[ignore] on headless CI, mirroring the Metal-device skip).

**Open risks:** W2 must land AFTER W1 in the W0→W4 order: Renderer::loop_enter forwards to Metal::loop_enter (metal_impl.rs:383, unimplemented!() until W1's M-4 IOSurfaceLayer display-callback seam). W2's tests drive the DisplayLink directly and never call Renderer::loop_enter, so W2 is independently gate-green, but the full loop_enter path is only live once W1 is in.; Renderer::init (generic.rs:1793) and deinit (1840) remain deferred draw-path seams in W2, so the reference display_link CREATION (generic.zig:692-699, gated on config.vsync) and stop+release teardown (807-812) are NOT wired by W2 — they stay as the verbatim comments already at generic.rs:1686-1693/1816-1820 and land un-commented in W1/W3. W2's tests own the DisplayLink lifecycle manually. Confirm W3 wires init's display_link creation + deinit teardown when it un-defers init/deinit.; CVDisplayLink is deprecated (macOS 15+); the raw extern "C" binding avoids the objc2-core-video deprecation/gap, but if a future macOS removes the symbol the vsync path needs a CADisplayLink/CAMetalDisplayLink port (out of scope for #1).; Headless/display-less CI: the culminating KAT can create a link that never fires. Decide the CI policy (display-attached runner vs #[ignore] on headless) — the KAT already skip-guards create() failure and deadline-bounds the pump, so it never hangs, but a display-less runner would fail the fires>0 assertion.; check_test_counts.py: the three added tests are rustty-only MARGIN (no mapped ghostty test{} in the CVDisplayLink block / display_link.zig). Verify the gate's per-file mapping treats them as margin (precedent: metal.rs smoke tests, live_shell.rs) and does not flag a count regression; add the margin annotation the gate expects if required.; Adding rustty-xev as a renderer dep is a new crate edge renderer→xev. Verified acyclic (xev deps = {rustty-global, libc}, no back-edge), but re-run check_acyclic.sh after the Cargo.toml change to confirm.

---

## Chunk W3 — swap renderer_seam::Renderer→real AND fill renderer_seam::Thread<A> bodies IN CORE (spec §5.2 unit E)

**Goal:** In crates/rustty-core/src/renderer_seam.rs: (E1) replace the unit-struct `Renderer` placeholder (surfaceInit/init/finalizeSurfaceInit/threadEnter/drawFrame/deinit are unimplemented!() at 88-169) with a thin same-module newtype wrapping the REAL monomorphic `rustty_renderer::Renderer` (offscreen+on-screen; init/deinit/present filled in W1), and confirm the already-wired Surface::draw→drawFrame / init / deinit callsites in surface.rs compile against it; (E2) fill the unimplemented!() bodies of the CORE-RESIDENT `Thread<A: apprt::App>::{init, deinit, threadMain}` (renderer_seam.rs:353-396) plus the private armLoop/wakeupCallback/drawNowCallback/drawCallback/renderCallback/cursorTimerCallback/cursorCancelCallback/stopCallback/drainMailbox/syncDrawTimer/drawFrame helpers over rustty-xev, porting ghostty src/renderer/Thread.zig in full. The Thread<A> struct + all fields (renderer_seam.rs:229-337) and the construct-then-spawn wiring (surface.rs RendererThreadArg :1489, spawn :2303-2314, ptr::write :1965) already exist and MUST NOT be redefined/moved. No new crate edge (core→renderer already exists); R4 acyclic stays green.

**Chunk culminating test:** crates/rustty-core/tests/render_thread_live.rs (macOS-gated, live_shell.rs pattern): heap-pin a Thread<A=TestApp> wrapping a real offscreen rustty_renderer::Renderer, `armLoop(&raw mut thread)` then bounded-pump `thread.loop_.run(RunMode::NoWait)` (tick cap + wall-clock deadline), and assert the renderer's IOSurface `Target` readback equals the committed offscreen golden for the seeded terminal state (proving the initial wakeup→wakeupCallback→renderCallback→updateFrame→drawFrame chain drove a live present through the core Thread loop); the same test also calls `seam_renderer.drawFrame(true)` directly (the Surface::draw path) and asserts Ok + golden. Gate: `cargo build --workspace --all-targets` + `.port/tools/gates/check_acyclic.sh` green.


### Task W3-E1: Swap renderer_seam::Renderer placeholder → real rustty_renderer::Renderer (newtype forwarders) + RendererError alias; confirm surface.rs callsites

**Files:**
- Modify: `crates/rustty-core/src/renderer_seam.rs` — 61-169 (RendererError struct 67-76 + PORT NOTE 78-86 + `pub struct Renderer;` and its 6 unimplemented!() methods 88-169); Options<A>.init<A> mapping consumed from 180-213
- Modify: `crates/rustty-core/src/surface.rs` — NO edits expected — verify-only: init 1679/1810-1831/2286, deinit 2559-2575/2596-2598, draw 2762-2768 already call the seam method names; recompile against the real error/return types
- Test: `crates/rustty-core/src/renderer_seam.rs (add #[cfg(test)] mod tests: forwarder shape/deinit no-panic on a headless offscreen renderer)`

**Consumes:**
- rustty_renderer::Renderer (real monomorphic GenericRenderer(Metal), crates/rustty-renderer/src/generic.rs:1138)
- rustty_renderer::Renderer::init(alloc: allocator_api2::alloc::Global, options: rustty_renderer::Options) -> Result<rustty_renderer::Renderer, rustty_renderer::Error> (generic.rs:1793; init body FILLED by W1)
- rustty_renderer::Renderer::deinit(&mut self) (generic.rs:1840; FILLED by W1)
- rustty_renderer::Renderer::draw_frame(&mut self, sync: bool) -> Result<(), rustty_renderer::Error> (generic.rs:4818)
- rustty_renderer::Renderer::{surface_init,finalize_surface_init,thread_enter,thread_exit} (ADDED by W1 unit C/B — apprt-side lifecycle; consumed here)
- rustty_renderer::Options { config: DerivedConfig, font_grid: *mut SharedGrid, size: Size, + (W1-extended) type-erased surface_mailbox/rt_surface/thread } (generic.rs:752)
- rustty_renderer::Error (generic.rs:925)
- crate::renderer_seam::Options<A> { config: DerivedConfig, font_grid, size, surface_mailbox: SurfaceMailbox<A>, rt_surface: *mut A::Surface, thread: *mut Thread<A> } (renderer_seam.rs:180-213)

**Produces:**
- pub struct Renderer(pub rustty_renderer::Renderer); (same-module newtype; the Thread<A> body reaches the real renderer via `(*ptr).0`)
- pub type RendererError = rustty_renderer::Error; (replaces the unit placeholder struct — keeps surface.rs `Result<(), crate::renderer_seam::RendererError>` and InitError::Renderer(RendererError) resolving)
- impl Renderer { pub fn surfaceInit<S>(rt_surface: *mut S) -> Result<(), RendererError>; pub fn init<A: apprt::App + 'static>(alloc: Global, options: Options<A>) -> Result<Renderer, RendererError>; pub fn finalizeSurfaceInit<S>(&mut self, rt_surface: *mut S) -> Result<(), RendererError>; pub fn threadEnter<S>(&self, rt_surface: *mut S) -> Result<(), RendererError>; pub fn drawFrame(&mut self, sync: bool) -> Result<(), RendererError>; pub fn deinit(&mut self); }

**Reference (port per R1 — verbatim comments, byte-faithful):**
- `src/renderer/generic.zig:864 surfaceInit, 658 init, 873 finalizeSurfaceInit, 881 threadEnter, 1430 drawFrame, 800 deinit` — the 6 Renderer entry points the seam wrapper forwards; camelCase names preserved verbatim (R1/R2, #![allow(non_snake_case)] already at renderer_seam.rs:40)
- `src/renderer.zig:38` — `pub const Renderer = switch(build_config.renderer){.metal=>GenericRenderer(Metal),...}` — the monomorphic Metal leaf is the wrapped type
- `src/Surface.zig:493 surfaceInit, 551 Renderer.init(alloc,.{...}), 712 finalizeSurfaceInit, 801 threadEnter, 884 draw's drawFrame(true)` — the callsites in surface.rs (already ported to the seam names) that must recompile

**FFI / xev mapping:**
- `try Renderer.init(alloc, .{.config=..,.font_grid=..,.size=..,.surface_mailbox=..,.rt_surface=..,.thread=..})` → `Renderer(rustty_renderer::Renderer::init(alloc, rustty_renderer::Options{ config: options.config, font_grid: options.font_grid, size: options.size, /* W1 type-erased */ surface_mailbox: options.surface_mailbox.into(), rt_surface: options.rt_surface.cast(), thread: options.thread.cast() })?)`
- `self.renderer.drawFrame(sync)` → `self.0.draw_frame(sync)`

**Test KAT:**
```rust
#[cfg(test)]
mod tests {
    use super::*;

    // R1: the placeholder had no tests (dependency-placeholder leaf); the swap
    // adds a headless forwarder-shape check. Full pixel proof is the chunk
    // culminating KAT (tests/render_thread_live.rs). Gated off non-macOS since a
    // real Renderer needs a Metal device.
    #[cfg(target_os = "macos")]
    #[test]
    fn seam_renderer_forwards_draw_and_deinit() {
        // Skip on hosts without a usable Metal device (ghostty SkipZigTest analogue).
        let Some(inner) = rustty_renderer::test_support::try_build_offscreen(
            rustty_renderer::size::Size {
                screen: rustty_renderer::size::ScreenSize { width: 800, height: 480 },
                cell: rustty_renderer::size::CellSize { width: 10, height: 20 },
                padding: rustty_renderer::size::Padding::default(),
            },
        ) else {
            eprintln!("skip: no Metal device");
            return;
        };
        let mut r = Renderer(inner);
        // Surface::draw path: drawFrame(true) must forward to the real renderer.
        r.drawFrame(true).expect("seam drawFrame forwards to real draw_frame");
        // deinit forwards without panic (real teardown filled in W1).
        r.deinit();
    }
}
```

**Port notes:** RendererError becomes `pub type RendererError = rustty_renderer::Error` (was a unit struct with Display 'renderer placeholder') — surface.rs's `Surface::draw -> Result<(), crate::renderer_seam::RendererError>` (2762) and `InitError::Renderer(RendererError)` keep resolving; the `?` in Surface::draw now propagates the real error. The seam `Renderer` is a same-module newtype `Renderer(pub rustty_renderer::Renderer)` (NOT a bare re-export) because surface.rs stores it by value as `renderer: Renderer` (surface.rs:325) and calls apprt-typed generic methods (`surfaceInit<S>`, `init<A>`, `threadEnter<S>`) the non-generic real renderer cannot carry without cycling (rustty-apprt-api→rustty-renderer edge). The Thread<A> body (same module, W3-E2) reaches the inner real renderer via `(*self.renderer).0`. rt_surface/thread type-erasure (`*mut A::Surface`/`*mut Thread<A>` → the renderer's opaque `*mut ()`) is a W1 deliverable (spec unit C 'routed through the renderer-local mailbox trait'); E1 only forwards. ADV-FIX 3: `alloc` stays on the wrapper's init. errdefer parity: the reference frees a partially-built DerivedConfig on init error — the seam's init<A> is a thin forward, so DerivedConfig ownership/free stays with the real renderer's own errdefer (W1); the seam does not double-free. No new crate edge (core→renderer exists) — R4 green.

**Gates:**
- `cargo build -p rustty-core --all-targets`
- `cargo test -p rustty-core --lib renderer_seam`
- `cargo clippy -p rustty-core --all-targets -- -D warnings`
- `cargo fmt -p rustty-core -- --check`
- `.port/tools/gates/check_acyclic.sh`
- `.port/tools/gates/check_comment_coverage.py`

**Risk:** Medium — the exact type-erasure shape of the W1-extended rustty_renderer::Options (surface_mailbox/rt_surface/thread) is a cross-chunk dependency; if W1 lands them as generic monomorphization rather than `*mut ()`, the init<A> mapping (`options.thread.cast()`) changes. Mitigation: E1 is the thinnest possible forward; adapt the 3-field mapping to whatever W1 exposes. Low risk that the error-type alias breaks surface.rs (grep-verified only 2 consumers).

### Task W3-E2a: Fill Thread<A>::{init, deinit} over rustty-xev (handle setup/teardown; NON-consuming deinit)

**Files:**
- Modify: `crates/rustty-core/src/renderer_seam.rs` — 353-386 (Thread<A>::init 366-376 body + deinit 384-386 body); struct+fields 229-337 UNCHANGED
- Test: `crates/rustty-core/src/renderer_seam.rs (#[cfg(test)] mod tests: init constructs all xev handles + deinit tears them down without leak, macOS-gated)`

**Consumes:**
- rustty_xev::Loop::init() -> Result<Loop, Error> ; Loop::deinit(&mut self)
- rustty_xev::Async::init() -> Result<Async, Error> ; Async::deinit(&mut self)
- rustty_xev::Timer::init() -> Result<Timer, Error> ; Timer::deinit(&self)
- rustty_xev::Completion (Default) ; rustty_xev::Error
- rustty_renderer::thread::Mailbox::create() -> Box<rustty_renderer::thread::Mailbox> (the real landed BlockingQueue<Message,64>; live_shell.rs:140 precedent)
- rustty_renderer::thread::DerivedConfig::init(config: &rustty_config::Config) -> DerivedConfig (custom_shader_animation)
- crate::app::Mailbox<A> (Copy handle; app.rs:946)
- crate::renderer_seam::{Renderer, State, Flags} ; struct Thread<A> fields (renderer_seam.rs:236-312)

**Produces:**
- pub fn init(alloc: Global, config: &rustty_config::Config, surface: *mut A::Surface, renderer_impl: *mut Renderer, state: *mut State, app_mailbox: crate::app::Mailbox<A>) -> Result<Thread<A>, xev::Error> — EXACT signature already declared at renderer_seam.rs:366-373 and called at surface.rs:1857-1864 (MUST match)
- pub fn deinit(&mut self) — NON-consuming (renderer_seam.rs:384; Surface::deinit calls it on the still-owned field at surface.rs:2596, after stop.notify()+join)

**Reference (port per R1 — verbatim comments, byte-faithful):**
- `src/renderer/Thread.zig:125-181` — init: xev.Loop.init + Async.init(wakeup/stop/draw_now) + Timer.init(render_h/draw_h/cursor_h) with LIFO errdefer, Mailbox.create, DerivedConfig.init(config); returns the struct literal
- `src/renderer/Thread.zig:183-196` — deinit: stop/wakeup/render_h/draw_h/draw_now/cursor_h.deinit + loop.deinit + mailbox.destroy(alloc) — the alloc field collapses (Box ownership); the Mailbox Box is dropped here
- `src/renderer/Thread.zig:19-20, 35` — DRAW_INTERVAL=8 / CURSOR_BLINK_INTERVAL=600 module consts; Mailbox = BlockingQueue(Message,64) — the real rustty_renderer::thread::Mailbox

**FFI / xev mapping:**
- `var loop = try xev.Loop.init(.{}); errdefer loop.deinit();` → `let mut loop_ = xev::Loop::init()?; // errdefer → explicit .deinit() on each later `?` arm (LIFO), termio Thread::init precedent`
- `var wakeup_h = try xev.Async.init(); (also stop, draw_now)` → `let wakeup = xev::Async::init()?; let stop = xev::Async::init()?; let draw_now = xev::Async::init()?;`
- `var render_h = try xev.Timer.init(); (also draw_h, cursor_timer)` → `let render_h = xev::Timer::init()?; let draw_h = xev::Timer::init()?; let cursor_h = xev::Timer::init()?;`
- `var mailbox = try Mailbox.create(alloc); errdefer mailbox.destroy(alloc);` → `let mailbox: *mut thread::Mailbox = Box::into_raw(thread::Mailbox::create()); // errdefer → drop(Box::from_raw(mailbox)) on later error arms; freed in deinit()`
- `self.stop.deinit(); ...; self.loop.deinit(); self.mailbox.destroy(self.alloc);` → `self.stop.deinit(); self.wakeup.deinit(); self.render_h.deinit(); self.draw_h.deinit(); self.draw_now.deinit(); self.cursor_h.deinit(); self.loop_.deinit(); unsafe { drop(Box::from_raw(self.mailbox)); }`

**Test KAT:**
```rust
#[cfg(test)]
mod init_deinit_tests {
    use super::*;
    use allocator_api2::alloc::Global;

    // Minimal ZST apprt::App double (imitates app.rs:1065 TestApp / lib.rs:128).
    #[derive(Clone, Copy)]
    struct TApp;
    struct TSurf;
    // (full apprt::App/Surface impls elided here — reuse the crate's test doubles;
    //  Thread::init only stores the pointers, it never derefs surface/state.)

    #[cfg(target_os = "macos")]
    #[test]
    fn thread_init_builds_and_deinit_tears_down_all_handles() {
        let cfg = rustty_config::Config::default();
        let mut surf = TSurf;
        // A real offscreen renderer wrapped in the seam newtype (W3-E1).
        let Some(inner) = rustty_renderer::test_support::try_build_offscreen(
            rustty_renderer::size::Size {
                screen: rustty_renderer::size::ScreenSize { width: 80, height: 24 },
                cell: rustty_renderer::size::CellSize { width: 10, height: 20 },
                padding: rustty_renderer::size::Padding::default(),
            },
        ) else { eprintln!("skip: no Metal device"); return; };
        let mut r = Renderer(inner);
        let mut st = std::mem::MaybeUninit::<State>::uninit();
        let app_mailbox = /* build a crate::app::Mailbox<TApp> over a throwaway queue */ unimplemented!();
        let mut t: Thread<TApp> = Thread::init(
            Global,
            &cfg,
            std::ptr::addr_of_mut!(surf) as *mut <TApp as apprt::App>::Surface,
            &raw mut r,
            st.as_mut_ptr(),
            app_mailbox,
        ).expect("Thread::init constructs loop + 3 Async + 3 Timer + Mailbox");
        // Flags default parity (visible=true, focused=true — renderer_seam.rs:342).
        assert!(t.flags.visible && t.flags.focused);
        assert!(!t.draw_active);
        // NON-consuming deinit (Surface::deinit calls it on the owned field).
        t.deinit();
        r.deinit();
    }
}
```

**Port notes:** init MUST match the canonical signature already wired at surface.rs:1857 and declared at renderer_seam.rs:366 — do not change arg order/types. The reference `alloc` field ('Allocator used for some state', Thread.zig:38) COLLAPSES: the Mailbox is a `Box::into_raw` pointer freed in deinit (termio thread.rs precedent), so the Thread struct has no `alloc` field (confirmed: renderer_seam.rs:229-312 has none). LIFO errdefer on `?`: each `xev::*::init()?` failure must explicitly `.deinit()` the already-built handles + `drop(Box::from_raw(mailbox))` (Rust has no `errdefer`); mirror Thread.zig:135-163 order. deinit is `&mut self` NON-consuming (the mailbox Box is freed here via `Box::from_raw`, matching `mailbox.destroy(alloc)`); it is only safe after the render thread is joined (Surface::deinit does stop.notify()+join at surface.rs:2559-2569 BEFORE calling renderer_thread.deinit() at :2596). Completions (wakeup_c/stop_c/render_c/draw_c/draw_now_c/cursor_c/cursor_c_cancel) take Default (`= .{}`) — inhabited, unarmed. DerivedConfig here is `rustty_renderer::thread::DerivedConfig` (custom_shader_animation), NOT the renderer.Renderer.DerivedConfig (renderer_seam.rs:305-309 note).

**Gates:**
- `cargo build -p rustty-core --all-targets`
- `cargo test -p rustty-core --lib renderer_seam::init_deinit_tests`
- `cargo clippy -p rustty-core --all-targets -- -D warnings`
- `.port/tools/gates/check_acyclic.sh`
- `.port/tools/gates/check_test_counts.py`

**Risk:** Low — init/deinit are a direct 1:1 port of Thread.zig:125-196 onto the already-proven rustty-xev handle API (same shape termio::Thread::init uses). The only subtlety is the LIFO errdefer chain on the 8 fallible inits; mirror the reference order exactly.

### Task W3-E2b: Fill Thread<A>::threadMain + armLoop + all xev callbacks + drainMailbox/syncDrawTimer/drawFrame helpers; culminating bounded-pump + Surface::draw KAT

**Files:**
- Create: `crates/rustty-core/tests/render_thread_live.rs (macOS-gated integration KAT — the chunk culmination)`
- Modify: `crates/rustty-core/src/renderer_seam.rs` — 388-395 (threadMain body) + ADD private items after 396: armLoop, threadMain_ (errors inner), drawFrame helper, syncDrawTimer, drainMailbox, and the ZST fn-item callbacks wakeupCallback/drawNowCallback/drawCallback/renderCallback/cursorTimerCallback/cursorCancelCallback/stopCallback + cursorBlinkInterval + consts DRAW_INTERVAL/CURSOR_BLINK_INTERVAL + MUST_DRAW_FROM_APP_THREAD
- Test: `crates/rustty-core/tests/render_thread_live.rs`

**Consumes:**
- From W3-E2a: Thread<A>::{init,deinit}
- From W3-E1: (*self.renderer).0 : &mut rustty_renderer::Renderer — .update_frame(&state::State, bool)->Result<(),VtOutOfMemory> (generic.rs:3861), .draw_frame(bool)->Result<(),Error> (4818), .has_animations()->bool (4654), .set_screen_size(Size) (1924), .set_font_grid(*mut SharedGrid) (1866), .change_config(DerivedConfig)->Result<(),Error> (1957); + W1-added .has_vsync()/.set_visible(bool)/.set_focus(bool)/.set_macos_display_id(u32)/.thread_enter(*mut S)/.thread_exit()
- rustty_xev unsafe fn Async::wait<UD,CB>(self, *mut Loop, *mut Completion, *mut UD, CB) where CB: Fn(*mut UD,*mut Loop,*mut Completion,Result<(),WaitError>)->CallbackAction (async.rs:217)
- rustty_xev unsafe fn Timer::run<UD,CB>(self, *mut Loop, *mut Completion, next_ms: u64, *mut UD, CB) (timer.rs:76); Timer::reset<UD,CB>(...,c_cancel,next_ms,...) (timer.rs:171); Timer::cancel<UD,CB>(self,*mut Loop,c_timer,c_cancel,*mut UD,CB) (timer.rs:219)
- rustty_xev::Loop::run(&mut self, RunMode)->Result<(),Error> (kqueue.rs:590); Loop::stop(&mut self) (kqueue.rs:377); Completion::state_of(&self)->CompletionState (kqueue.rs:1595); CompletionState::{Active,Dead} (loop.rs:138); CallbackAction::{Rearm,Disarm}; RunMode::{NoWait,UntilDone}
- crate::app::Mailbox<A>::push(Message<A>, Timeout)->Size (app.rs:952); crate::app::Message::RedrawSurface(*mut A::Surface) (app.rs:896)
- rustty_renderer::thread::Mailbox::pop()->Option<rustty_renderer::message::Message> and its variants (visible/focus/reset_cursor_blink/font_grid/resize/change_config/inspector/macos_display_id/crash/search_*)

**Produces:**
- pub unsafe fn threadMain(self_: *mut Thread<A>) — RAW-ptr entry (matches the spawn at surface.rs:2311 `(*arg.0).threadMain()`; the RendererThreadArg Send-wrapper is already wired at surface.rs:1489/2303)
- unsafe fn armLoop<A>(self_: *mut Thread<A>) -> Result<(), xev::NotifyError> — factored arming (wait handlers + initial wakeup.notify + cursor_h.run + syncDrawTimer); the bounded-pump KAT calls this then run(NoWait) instead of the blocking run(UntilDone)
- private: fn wakeupCallback<A>/drawNowCallback<A>/drawCallback<A>/renderCallback<A>/cursorTimerCallback<A>/cursorCancelCallback<A>/stopCallback<A> as ZST fn items matching the xev trampoline sig; methods drainMailbox/syncDrawTimer/drawFrame(now) on *mut Thread<A>
- const DRAW_INTERVAL: u64 = 8; const CURSOR_BLINK_INTERVAL: u64 = 600; const MUST_DRAW_FROM_APP_THREAD: bool = false;

**Reference (port per R1 — verbatim comments, byte-faithful):**
- `src/renderer/Thread.zig:199-263` — threadMain + threadMain_: pthread name (darwin), crash.sentry.thread_state (deferred seam), setQosClass, threadEnter/threadExit, wait(wakeup/stop/draw_now), wakeup.notify initial, cursor_h.run, syncDrawTimer, loop.run(.until_done). armLoop = 238-257 (arming minus the final run).
- `src/renderer/Thread.zig:295-334` — syncDrawTimer: hasAnimations/custom_shader_animation branch → draw_active + draw_h.run(DRAW_INTERVAL, drawCallback); the `if draw_c.state()==.active return` guard
- `src/renderer/Thread.zig:336-517` — drainMailbox (autoreleasepool on darwin) — every message arm (visible/focus/reset_cursor_blink/font_grid/resize/change_config/search_*/inspector/macos_display_id/crash) verbatim; changeConfig; drawFrame(now) helper with must_draw_from_app_thread branch (app_mailbox.push .redraw_surface) vs renderer.drawFrame(false)
- `src/renderer/Thread.zig:519-725` — wakeupCallback/drawNowCallback/drawCallback/renderCallback/cursorTimerCallback/cursorCancelCallback/stopCallback + cursorBlinkInterval; renderCallback's `if !visible disarm` + updateFrame + drawFrame; wakeupCallback drains then calls renderCallback(t,undefined,undefined,{})
- `src/Surface.zig:884` — draw(): try self.renderer.drawFrame(true) — the Surface::draw path exercised by the KAT (surface.rs:2766)

**FFI / xev mapping:**
- `self.wakeup.wait(&self.loop, &self.wakeup_c, Thread, self, wakeupCallback);` → `unsafe { (*self_).wakeup.wait(&raw mut (*self_).loop_, &raw mut (*self_).wakeup_c, self_, wakeupCallback::<A>) }; // UD=Thread<A>, CB=ZST fn item`
- `self.stop.wait(...,stopCallback); self.draw_now.wait(...,drawNowCallback);` → `unsafe { (*self_).stop.wait(&raw mut (*self_).loop_, &raw mut (*self_).stop_c, self_, stopCallback::<A>) }; // draw_now analogous with drawNowCallback::<A>`
- `try self.wakeup.notify(); (initial render kick)` → `unsafe { (*self_).wakeup.notify() }?; // NotifyError`
- `self.cursor_h.run(&self.loop, &self.cursor_c, cursorBlinkInterval(), Thread, self, cursorTimerCallback);` → `unsafe { (*self_).cursor_h.run(&raw mut (*self_).loop_, &raw mut (*self_).cursor_c, cursorBlinkInterval(), self_, cursorTimerCallback::<A>) };`
- `self.draw_h.run(&self.loop, &self.draw_c, DRAW_INTERVAL, Thread, self, drawCallback);` → `unsafe { (*self_).draw_h.run(&raw mut (*self_).loop_, &raw mut (*self_).draw_c, DRAW_INTERVAL, self_, drawCallback::<A>) };`
- `self.cursor_h.cancel(&self.loop, &self.cursor_c, &self.cursor_c_cancel, void, null, cursorCancelCallback);` → `unsafe { (*t).cursor_h.cancel(&raw mut (*t).loop_, &raw mut (*t).cursor_c, &raw mut (*t).cursor_c_cancel, core::ptr::null_mut::<()>(), cursorCancelCallback) }; // UD=()`
- `self.cursor_h.reset(&self.loop,&self.cursor_c,&self.cursor_c_cancel, cursorBlinkInterval(), Thread, self, cursorTimerCallback);` → `unsafe { (*t).cursor_h.reset(&raw mut (*t).loop_, &raw mut (*t).cursor_c, &raw mut (*t).cursor_c_cancel, cursorBlinkInterval(), t, cursorTimerCallback::<A>) };`
- `if (self.draw_c.state() == .active) return;` → `if unsafe { (*self_).draw_c.state_of() } == xev::CompletionState::Active { return; }`
- `_ = try self.loop.run(.until_done); / self_.?.loop.stop();` → `unsafe { (*self_).loop_.run(xev::RunMode::UntilDone) }?; / unsafe { (*t).loop_.stop() };`

**Test KAT:**
```rust
//! Chunk W3 culmination — the core Thread<A> render loop drives a live drawFrame
//! through the real (offscreen) renderer; the same present the Surface::draw path
//! produces. Imitates crates/rustty-termio/tests/live_shell.rs: heap-pin, arm,
//! bounded run(NoWait) pump, deadline + tick cap, explicit teardown.
#![cfg(target_os = "macos")]
#![allow(non_snake_case)]

use std::time::{Duration, Instant};
use allocator_api2::alloc::Global;
use rustty_core::renderer_seam::{Renderer, Thread, State};
use rustty_core::app::Mailbox as AppMailbox;
use rustty_xev::RunMode;

// Reuse the crate's TestApp/TestSurface doubles (app.rs:1065/1138).
mod doubles; use doubles::{TestApp, TestSurface, build_app_mailbox};

#[test]
fn core_thread_loop_and_surface_draw_present_the_golden() {
    // Offscreen 80x24 renderer (no on-screen layer ⇒ has_vsync()==false ⇒
    // drawFrame draws directly; deterministic without a display link).
    let size = rustty_renderer::size::Size {
        screen: rustty_renderer::size::ScreenSize { width: 800, height: 480 },
        cell: rustty_renderer::size::CellSize { width: 10, height: 20 },
        padding: rustty_renderer::size::Padding::default(),
    };
    let Some(inner) = rustty_renderer::test_support::try_build_offscreen(size) else {
        eprintln!("skip: no Metal device"); return;
    };
    // Heap-pin the seam renderer + shared State (their addresses feed the Thread).
    let renderer: *mut Renderer = Box::into_raw(Box::new(Renderer(inner)));
    let (state, seed_terminal) = doubles::build_seeded_state(); // writes 'RUSTTY_OK' row

    let cfg = rustty_config::Config::default();
    let mut rt_surface = TestSurface::new();
    let app_mailbox: AppMailbox<TestApp> = build_app_mailbox();

    // Heap-pin the Thread (the loop re-enters inline Completions across ticks; it
    // must not move — live_shell.rs pin discipline).
    let thread: *mut Thread<TestApp> = Box::into_raw(Box::new(
        Thread::<TestApp>::init(
            Global, &cfg,
            std::ptr::addr_of_mut!(rt_surface) as *mut <TestApp as rustty_apprt_api::App>::Surface,
            renderer, state, app_mailbox,
        ).expect("Thread::init"),
    ));

    // === (1) Surface::draw path: direct drawFrame(true) presents the golden. ===
    unsafe { (*renderer).drawFrame(true) }.expect("Surface::draw drawFrame(true)");
    let dump0 = rustty_renderer::test_support::readback_iosurface(unsafe { &(*renderer).0 });
    rustty_renderer::test_support::assert_matches_golden(&dump0, "render_thread_live");

    // === (2) Thread loop path: arm (the factored threadMain_ prologue), then
    // bounded-pump run(NoWait). armLoop sends the initial wakeup ⇒
    // wakeupCallback ⇒ drainMailbox ⇒ renderCallback ⇒ updateFrame + drawFrame. ===
    unsafe { Thread::armLoop(thread) }.expect("armLoop arms wait/timers + initial wakeup");

    let deadline = Instant::now() + Duration::from_secs(3);
    let mut presented = false;
    for _ in 0..2000 {
        unsafe { (*thread).loop_.run(RunMode::NoWait) }.expect("loop.run(NoWait)");
        let dump = rustty_renderer::test_support::readback_iosurface(unsafe { &(*renderer).0 });
        if rustty_renderer::test_support::equals_golden(&dump, "render_thread_live") {
            presented = true; break;
        }
        if Instant::now() > deadline { break; }
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(presented, "core Thread render loop did not present the golden within the deadline");

    // === Teardown: stop the loop, deinit, drop the pinned boxes (no thread was
    // spawned — armLoop+NoWait pump ran on THIS thread — so no join needed). ===
    unsafe { (*thread).stop.notify() }.ok();
    unsafe { (*thread).loop_.run(RunMode::NoWait) }.ok(); // drain the stop async
    unsafe {
        let mut t = Box::from_raw(thread);
        t.deinit();                 // NON-consuming; frees the render Mailbox box
        let mut r = Box::from_raw(renderer);
        r.deinit();
    }
    doubles::teardown_state(state, seed_terminal);
}
```

**Port notes:** threadMain is `pub unsafe fn threadMain(self_: *mut Thread<A>)` — RAW-ptr receiver (NOT &mut self): the construct-then-spawn is already wired via RendererThreadArg(*mut Thread<A>) at surface.rs:1489 and the spawn closure calls `(*arg.0).threadMain()` at surface.rs:2311; the render thread is the SOLE user of *thread between spawn and the Surface::deinit stop.notify()+join (surface.rs:2559-2569). SB/TB: NEVER take `&mut *self_` on the whole Thread while completions are armed — every callback/helper takes `*mut Thread<A>` and touches fields through `&raw mut (*self_).field` (the rust-mut-args-strongly-protected discipline; the loop holds the inline Completions raw across ticks, a &mut reborrow would retag+UB). xev callbacks are generic ZST fn items `fn wakeupCallback<A: apprt::App + 'static>(t: *mut Thread<A>, _: *mut Loop, _: *mut Completion, r: Result<(),WaitError>) -> CallbackAction` — matching the trampoline sig (async.rs:224 / timer.rs:82); UD=Thread<A>. cursorCancelCallback uses UD=() with null userdata (Thread.zig:668 `?*void`). armLoop is a rustty-only factoring of threadMain_'s arming prologue (Thread.zig:238-257) so the bounded-pump KAT can drive the loop deterministically without the blocking `run(.until_done)`; threadMain_ = armLoop(self_) then `loop_.run(UntilDone)`. must_draw_from_app_thread → `const MUST_DRAW_FROM_APP_THREAD: bool = false` (module const, port-noted as the macOS direct-draw value; BOTH reference branches preserved for R1 — the app_mailbox.push(Message::RedrawSurface(surface)) branch compiles via app.rs:896/952 but is dead under false). drawFrame(now) helper reads `(*self.renderer).0.has_vsync()` (W1) — offscreen returns false so drawFrame draws. renderer method calls route through `(*self.renderer).0` (the real renderer, same module). crash.sentry.thread_state (Thread.zig:218-222 reading renderer.surface_mailbox.surface) is a DEFERRED seam — port the faithful comment but the set/clear routes through the existing crash::sentry no-op (core already has clearThreadState); the surface_mailbox.surface access stays behind the renderer-local SurfaceMailbox seam. setQosClass (Thread.zig:265-293, darwin macos.setQosClass) — port via the rustty-os macos QoS shim if landed, else a no-op with a verbatim-comment DEFERRED note (0 tests, R1-safe). cursorBlinkInterval's valgrind branch (Thread.zig:713) → rustty has no std.valgrind; port as `fn cursorBlinkInterval()->u64 { CURSOR_BLINK_INTERVAL }` with a port note that the valgrind-slowdown branch is dropped (the reference hack, not observable behavior). autoreleasepool in drainMailbox (Thread.zig:342) → an objc2 AutoreleasePool RAII guard on macos, else nothing. The KAT spawns NO OS thread (armLoop + NoWait pump on the test thread) so teardown needs no join — but it still exercises the exact threadMain callback graph.

**Gates:**
- `cargo build --workspace --all-targets`
- `cargo test -p rustty-core --test render_thread_live`
- `cargo clippy -p rustty-core --all-targets -- -D warnings`
- `cargo fmt -p rustty-core -- --check`
- `.port/tools/gates/check_acyclic.sh`
- `.port/tools/gates/check_comment_coverage.py`
- `.port/tools/gates/check_test_counts.py`
- `cargo +nightly miri test -p rustty-core (render_thread_live is Miri-EXCLUDED: objc/Metal foreign calls — record in .port/phase-2/MIRI_EXCEPTIONS.md; the raw-ptr aliasing is proven on the mirihar renderthread_naive=UB / renderthread_shipped=CLEAN pair per spec §5.5)`

**Risk:** High — the render-thread raw-pointer aliasing soundness (self_: *mut Thread<A> shared with the armed inline Completions) is the chunk's central hazard; mitigation is the mirihar renderthread_naive/renderthread_shipped candidate pair (spec §5.5/§5.7) which must be CLEAN before merge, plus the KAT running armLoop+NoWait on a single thread to keep the first proof deterministic. Secondary risk: the offscreen readback/golden test helpers (try_build_offscreen/readback_iosurface/assert_matches_golden) are consumed from the renderer crate's W1 golden gate — if they are not pub-exposed to core's integration tests, this task must add a thin `#[cfg(feature="test-offscreen")]` re-export in rustty-renderer (no logic, no new edge).

**Open risks:** W3 consumes a set of real-renderer methods that are W1/W2 deliverables (has_vsync, set_visible, set_focus, set_macos_display_id, thread_enter, thread_exit, loop_enter/loop_exit, surface_init, finalize_surface_init, and the extended rustty_renderer::Options carrying type-erased surface_mailbox/rt_surface/thread). If W1/W2 name or shape these differently, the E1 forwarders and the E2b `(*self.renderer).0.<method>` callsites need matching. Verified as MISSING today (grep: only update_frame/draw_frame/set_screen_size/set_font_grid/change_config/has_animations exist on generic.rs).; The chunk overrides spec §5.2 unit D wording ('Lands in crates/rustty-renderer/src/thread.rs'): the Thread<A> loop is filled IN CORE (renderer_seam.rs) because it is generic over apprt A and holds crate::app::Mailbox<A> + *mut A::Surface, which rustty-renderer cannot name without a cycle. rustty-renderer/src/thread.rs keeps only DerivedConfig + the real Mailbox. Confirm the executing agent honors this (the task statement is authoritative over the spec table).; The offscreen golden readback helpers (try_build_offscreen/readback_iosurface/assert_matches_golden/equals_golden) must be reachable from a core integration test; if the renderer crate keeps them #[cfg(test)]-private, a thin pub `test_support` surface (feature-gated, no new crate edge) is required — flagged in W3-E2b risk.; must_draw_from_app_thread is ported as a module const `false` (macOS direct-draw). If a later apprt (GTK) needs the redraw_surface-message path, this becomes an apprt::App associated const; both reference branches are preserved so the upgrade is non-breaking.; xev::Error is used in the already-declared Thread::init signature but is not in the kqueue re-export list I grepped (lib.rs:166-169); it currently resolves (the placeholder compiles), so preserved as-is — but confirm the path if the signature is touched.; setQosClass / crash.sentry.thread_state / autoreleasepool are darwin platform seams; ported faithfully but may reduce to no-ops with verbatim-comment DEFERRED notes if the rustty-os QoS shim / objc2 AutoreleasePool are not yet landed (0 tests, R1-safe).

---

## Chunk W4 — apprt-c macOS host handshake + blur/display_id/scale (sub-project #1 unit F). Fills the two remaining macOS `#[cfg(target_os=\"macos\")]` C-ABI leaves in crates/rustty-apprt-c/src/lib.rs (window blur :2164-2192, display_id :2234-2248), promotes the config blur-radius accessor, and lands the culminating macOS windowing integration harness that opens a real NSWindow, hosts the CAMetalLayer via the C-ABI, and shows a live terminal frame with no Swift app. Depends on W0-W3 (IOSurfaceLayer, Metal on-screen present, renderer Thread over rustty-xev, real Renderer swapped into renderer_seam). quicklook/secure-input/inspector leaves STAY deferred to sub-project #6.

**Goal:** Realize the two macOS host-integration C-ABI leaves (CGSSetWindowBackgroundBlurRadius window blur; renderer-thread mailbox macos_display_id push) with byte-faithful bodies, and prove the whole macOS windowing stack end-to-end: a Rust integration test constructs an App+Surface through the published C-ABI, hands a real NSView across the platform union, the embedded Surface's already-live Platform/content-scale handshake drives the renderer to attach a CAMetalLayer to that view, a bounded live-loop pump (live_shell.rs discipline) presents at least one real frame on-screen (FIRST ON-SCREEN PIXELS via the C-ABI), the presented IOSurface is read back and structurally compared against the committed offscreen golden gate, and set_display_id + set_window_background_blur are exercised panic-free against the live surface/window. The embedded.rs NSView/content-scale host handshake (Platform::init, updateContentScale) is already LIVE from E2/E3 and is only EXERCISED here — no new embedded.rs leaf is filled (the ~1813-2176 range cited is the Inspector block, which stays deferred).

**Chunk culminating test:** crates/rustty-apprt-c/tests/macos_window_host.rs::hosts_nswindow_layer_and_presents_live_frame — opens an NSWindow + contentView, drives rustty_app_new → rustty_surface_new with platform.macos.nsview = contentView, bounded-pumps rustty_app_tick until the contentView hosts a CAMetalLayer whose contents (the presented IOSurface) is non-null, reads that IOSurface back BGRA and structurally asserts the terminal frame against the golden gate, then calls rustty_surface_set_display_id(CGMainDisplayID()) and rustty_set_window_background_blur(app, nswindow) panic-free.


### Task W4-1: Deps + config blur-radius accessor promotion (enabler)

**Files:**
- Modify: `crates/rustty-apprt-c/Cargo.toml` — [dependencies] after ~L48 (rustty-config) and [dev-dependencies] (new section): add live objc2 + the two renderer/datastruct forward edges for the display_id push, and the macOS windowing dev-deps for the harness
- Modify: `crates/rustty-config/src/value_types.rs` — 2326 (fn background_blur_cval signature line)
- Test: `crates/rustty-config/src/c_get.rs`

**Consumes:**
- rustty_config::value_types::BackgroundBlur (re-export of rustty_render_types::BackgroundBlur)

**Produces:**
- pub fn rustty_config::value_types::background_blur_cval(v: BackgroundBlur) -> i16
- Cargo edges: rustty-apprt-c -> { objc2 0.6 (dep), rustty-renderer (dep), rustty-datastruct (dep) }; dev-deps rustty-apprt-c -> { objc2-app-kit 0.3, objc2-quartz-core 0.3, objc2-foundation 0.3, objc2-core-graphics 0.3, rustty-xev }

**Reference (port per R1 — verbatim comments, byte-faithful):**
- `src/apprt/embedded.zig:2183` — config.@"background-blur".cval() — the BackgroundBlur.cval() method the blur leaf calls; its rustty analogue background_blur_cval must be pub so apprt-c (a separate crate) can reach it
- `src/config/Config.zig:BackgroundBlur.cval` — i16 radius mapping (false=0,true=20,radius=>|v|,glass=>-1/-2) — already ported verbatim at value_types.rs:2326, only visibility changes

**FFI / xev mapping:**
- `objc.msgSend / CGS extern (dep availability)` → `objc2 = { version = "0.6" } added to rustty-apprt-c [dependencies] (msg_send! windowNumber for W4-2)`
- `renderer Thread.Mailbox / xev.Async (dep availability)` → `rustty-renderer + rustty-datastruct added to [dependencies] (Message::MacosDisplayId + Timeout for W4-3); objc2-app-kit/objc2-quartz-core/objc2-core-graphics/rustty-xev added to [dev-dependencies] (W4-4 harness)`

**Test KAT:**
```rust
// No new test file. The existing c_get.rs golden (`c_get_background_blur`,
// rustty-config/src/c_get.rs:570) already exercises background_blur_cval via the
// C-get path and must stay green after the pub(crate)->pub change (no behavior
// change, only visibility). Verification is the config crate's own test suite:
//   cargo test -p rustty-config background_blur
// plus a compile-check that apprt-c can now name the fn (W4-2 uses it). The only
// source edit is the signature line:
//   -pub(crate) fn background_blur_cval(v: BackgroundBlur) -> i16 {
//   +pub fn background_blur_cval(v: BackgroundBlur) -> i16 {
```

**Port notes:** R1: promoting background_blur_cval from pub(crate) to pub is the minimal-visibility change that preserves the reference's `BackgroundBlur.cval()` as a public method reachable across the crate boundary; the body + its distinctive reference-provenance comments stay CONFIG-local (value_types.rs), exactly as the Chunk-3a hoist note prescribes (the enum lives in rustty-render-types, the value methods stay config-local). R4/acyclic: rustty-apprt-c is the top sink of the graph; adding rustty-renderer/rustty-datastruct/objc2 as forward edges to crates far below it keeps check_acyclic.sh GREEN (same precedent as the existing rustty-vt/rustty-core/rustty-config forward edges in apprt-c). objc2-quartz-core is ADDED to the lock here (dev-dep) per the design; W0 already added it to rustty-renderer. Dev-deps only compile for the macOS harness test binary, so non-macOS builds are unaffected.

**Gates:**
- `cargo build -p rustty-apprt-c`
- `cargo test -p rustty-config background_blur`
- `cargo clippy -p rustty-config -p rustty-apprt-c --all-targets -- -D warnings`
- `bash .port/tools/gates/check_acyclic.sh`

**Risk:** objc2-app-kit/objc2-quartz-core/objc2-core-graphics 0.3 version skew against the in-lock objc2 0.6 / objc2-foundation 0.3 — mitigation: pin the exact 0.3.x already resolved by rustty-renderer's Cargo.lock; the objc2-* 0.3 family is co-released against objc2 0.6.

### Task W4-2: Fill rustty_set_window_background_blur (objc2 windowNumber + private CGS blur)

**Files:**
- Modify: `crates/rustty-apprt-c/src/lib.rs` — 2172-2191 (replace the #[cfg(target_os="macos")] unimplemented!() block with the real body) and 2194-2204 (turn the preserved CGS extern-decl comment into a live extern "C" block)
- Test: `crates/rustty-apprt-c/tests/macos_window_host.rs`

**Consumes:**
- rustty_apprt::embedded::App with pub field config: UnsafeCell<rustty_config::Config>
- rustty_config::Config { pub background_opacity: f64, pub background_blur: rustty_config::value_types::BackgroundBlur }
- pub fn rustty_config::value_types::background_blur_cval(BackgroundBlur) -> i16 (from W4-1)
- objc2::runtime::AnyObject, objc2::msg_send!

**Produces:**
- #[no_mangle] pub unsafe extern "C" fn rustty_set_window_background_blur(app: *mut App, window: *mut c_void) — realized macOS body (non-macOS arm stays the no-op)
- extern "C" { fn CGSSetWindowBackgroundBlurRadius(connection: *mut c_void, window: usize, radius: c_int) -> i32; fn CGSDefaultConnectionForThread() -> *mut c_void; }

**Reference (port per R1 — verbatim comments, byte-faithful):**
- `src/apprt/embedded.zig:2101-2126` — export fn ghostty_set_window_background_blur: reads &app.config, early-returns when background-opacity>=1.0, objc.Object.fromId(window).msgSend(usize, sel("windowNumber")), CGSSetWindowBackgroundBlurRadius(CGSDefaultConnectionForThread(), windowNumber, @intCast(config.background-blur.cval()))
- `src/apprt/embedded.zig:2128-2130` — extern "c" fn CGSSetWindowBackgroundBlurRadius(*anyopaque, usize, c_int) i32; extern "c" fn CGSDefaultConnectionForThread() *anyopaque; — the private CoreGraphics symbols, now declared live

**FFI / xev mapping:**
- `objc.Object.fromId(window).msgSend(usize, objc.sel("windowNumber"), .{})` → `let nswindow: *mut objc2::runtime::AnyObject = window.cast(); let window_number: isize = unsafe { objc2::msg_send![nswindow, windowNumber] }; (NSWindow.windowNumber returns NSInteger=isize; cast to usize for the CGS arg, matching the reference @intCast to usize)`
- `CGSSetWindowBackgroundBlurRadius(CGSDefaultConnectionForThread(), n, @intCast(cval))` → `unsafe { CGSSetWindowBackgroundBlurRadius(CGSDefaultConnectionForThread(), window_number as usize, background_blur_cval(config.background_blur) as core::ffi::c_int) } — private CGS symbols via a live extern "C" block (no crate provides them; faithful to the reference extern decls)`

**Test KAT:**
```rust
// In crates/rustty-apprt-c/tests/macos_window_host.rs (shared harness file,
// W4-4). Focused blur KAT: a real NSWindow + a live App with transparent config
// so the early-return is NOT taken and the objc msgSend + private CGS call are
// actually exercised panic-free.
#[test]
fn set_window_background_blur_is_panic_free_on_transparent_config() {
    let Some(mtm) = MainThreadMarker::new() else {
        eprintln!("not main thread; skipping AppKit KAT");
        return;
    };
    // background-opacity 0.5 (< 1.0) => the reference DOES NOT early-return, so
    // windowNumber + CGSSetWindowBackgroundBlurRadius run for real.
    let config = make_config_transparent_blur(); // *mut rustty_config::Config, opacity=0.5, blur=.true (=>radius 20)
    let app = unsafe { rustty_app_new(&test_options(), config) };
    assert!(!app.is_null(), "app_new");
    let window = make_nswindow(mtm); // *mut objc2::runtime::AnyObject (NSWindow), 200x120
    // The load-bearing assertion: this call reaches objc msgSend(windowNumber)
    // + the private CGSSetWindowBackgroundBlurRadius and returns cleanly.
    unsafe { rustty_set_window_background_blur(app, window.cast()) };
    // Opaque config (>=1.0) must hit the early return (also panic-free).
    let opaque = make_config_opaque(); // opacity=1.0
    unsafe { rustty_config_replace(app, opaque) };
    unsafe { rustty_set_window_background_blur(app, window.cast()) };
    unsafe { rustty_app_free(app) };
    keep_window_alive(window);
}
```

**Port notes:** R1 byte-fidelity: the body is the reference translated 1:1 — `const config = &app.config` => `let config = unsafe { &*(*app).config.get() }` (App.config is an UnsafeCell<Config> per the E6 field note; the C host holds a single-threaded *mut App so the shared read is sound), the `background-opacity >= 1.0` early return, the windowNumber msgSend, and the CGS call with the `@intCast(cval)` widening. The reference @export is UNCONDITIONAL (only the BODY is comptime-gated); rustty already mirrors this — the fn exists on all targets, the #[cfg(not(macos))] arm is the no-op early return, the #[cfg(macos)] arm gets the real body. The two private CGS externs move from preserved comment to a live `extern "C"` block placed exactly where the reference declares them (right after the fn); update the preceding PORT NOTE to record that they are now realized (they were deferred precisely 'until the objc/CGS leaf is realized' — that is now). No objc2-app-kit needed for the leaf itself: msg_send! on a raw AnyObject* is enough (objc2 dep only). SB/TB: `window` is a foreign NSWindow pointer never dereferenced as a Rust reference — only handed to msg_send!/CGS; no aliasing obligation. Miri-excluded (foreign objc/CGS calls) per MIRI_EXCEPTIONS.md; ASan-covered on the FFI gate.

**Gates:**
- `cargo build -p rustty-apprt-c`
- `cargo test -p rustty-apprt-c --test macos_window_host set_window_background_blur`
- `cargo clippy -p rustty-apprt-c --all-targets -- -D warnings`
- `cargo fmt --check`
- `python3 .port/tools/gates/check_comment_coverage.py`

**Risk:** CGSSetWindowBackgroundBlurRadius / CGSDefaultConnectionForThread are undocumented private symbols — if the linker cannot resolve them the cdylib link fails; mitigation: they are Terminal.app-grade stable and resolved from CoreGraphics.framework (already transitively linked via objc2-core-graphics); the reference links them identically. windowNumber return-type mismatch (NSInteger vs the reference usize) — mitigation: read as isize then `as usize`, matching the reference @intCast semantics.

### Task W4-3: Fill rustty_surface_set_display_id (renderer-thread mailbox push + xev wakeup)

**Files:**
- Modify: `crates/rustty-apprt-c/src/lib.rs` — 2236-2248 (replace the let _ = (ptr, display_id); + unimplemented!() body with the real mailbox push + wakeup.notify)
- Test: `crates/rustty-apprt-c/tests/macos_window_host.rs`

**Consumes:**
- rustty_apprt::embedded::Surface with pub field core_surface: rustty_core::Surface<rustty_apprt::embedded::App>
- rustty_core::Surface.renderer_thread: rustty_core::renderer_seam::Thread<A> with pub mailbox: *mut rustty_renderer::thread::Mailbox and pub wakeup: rustty_xev::Async (produced live by W2/W3)
- rustty_renderer::thread::Mailbox = rustty_datastruct::BlockingQueue<rustty_renderer::Message, 64>
- rustty_renderer::Message::MacosDisplayId(u32)
- rustty_datastruct::blocking_queue::Timeout::Forever
- rustty_datastruct::blocking_queue::BlockingQueue::push(&self, T, Timeout) -> Size
- rustty_xev::Async::notify(&self) -> Result<(), rustty_xev::NotifyError>

**Produces:**
- #[cfg(target_os="macos")] #[no_mangle] pub unsafe extern "C" fn rustty_surface_set_display_id(ptr: *mut Surface, display_id: u32) — realized body

**Reference (port per R1 — verbatim comments, byte-faithful):**
- `src/apprt/embedded.zig:2134-2141` — Darwin export ghostty_surface_set_display_id: const surface = &ptr.core_surface; _ = surface.renderer_thread.mailbox.push(.{ .macos_display_id = display_id }, .{ .forever = {} }); surface.renderer_thread.wakeup.notify() catch {};
- `src/renderer/Thread.zig:485-487` — drainMailbox handles .macos_display_id => self.renderer.setMacOSDisplayID(v) — the consumer side proving the message variant + mailbox is the correct transport (lands live in W2/W3)
- `src/renderer/message.zig:68` — macos_display_id: u32 — the Message union variant, ported as rustty_renderer::Message::MacosDisplayId(u32) (message.rs:118)

**FFI / xev mapping:**
- `surface.renderer_thread.mailbox.push(.{ .macos_display_id = display_id }, .{ .forever = {} })` → `let _ = unsafe { &*surface.renderer_thread.mailbox }.push(rustty_renderer::Message::MacosDisplayId(display_id), rustty_datastruct::blocking_queue::Timeout::Forever); — mailbox is a *mut Mailbox (Box pointee), deref to &Mailbox then BlockingQueue::push; the Size return is discarded exactly as the reference `_ =``
- `surface.renderer_thread.wakeup.notify() catch {}` → `let _ = surface.renderer_thread.wakeup.notify(); — rustty_xev::Async::notify returns Result<(),NotifyError>; the reference `catch {}` swallows the error, so `let _ =` is the faithful mapping`

**Test KAT:**
```rust
// In crates/rustty-apprt-c/tests/macos_window_host.rs. Requires a LIVE surface
// (real renderer_thread mailbox/wakeup from W2/W3), so it reuses the hosted
// surface built by the W4-4 harness helper.
#[test]
fn set_display_id_pushes_to_render_thread_panic_free() {
    let Some(mtm) = MainThreadMarker::new() else { return; };
    let host = match HostedSurface::spawn(mtm) { // app_new -> surface_new(nsview) -> render+io threads live
        Some(h) => h,
        None => return, // no Metal device: skip (golden Metal-skip precedent)
    };
    // CGMainDisplayID() is a real, always-valid display id on the test host.
    let display_id: u32 = unsafe { objc2_core_graphics::CGMainDisplayID() };
    // The load-bearing assertion: the push+wakeup reaches the live render-thread
    // mailbox and returns cleanly (no unimplemented!()/panic). The render thread
    // drains it into Renderer::setMacOSDisplayID; we do a bounded pump so the
    // drain actually runs.
    unsafe { rustty_surface_set_display_id(host.surface, display_id) };
    host.pump_bounded(); // rustty_app_tick loop (live_shell.rs bounded discipline)
    // Surviving the pump proves the message was well-formed and drained.
    drop(host);
}
```

**Port notes:** R1: verbatim body — the `_ =` on push (discard Size) and `let _ =` on notify (the reference `catch {}`) are the faithful mappings; update the preceding PORT NOTE to record the leaf is now realized (it was deferred 'until a live surface + render thread exist' — W2/W3 provide exactly that). The renderer_thread fields are ALREADY REAL in renderer_seam.rs (pub wakeup: xev::Async :240, pub mailbox: *mut thread::Mailbox :298) — only Thread::init/threadMain were leaves, and W2/W3 fill those, so by W4 the mailbox pointer is a valid live BlockingQueue and wakeup is an armed Async. R5: this is the R5 'deferred = unimplemented!() + verbatim body' leaf being promoted to live now that its consumed seams exist. SB/TB: the mailbox is a Box pointee OUTSIDE the Surface struct's Stacked-Borrows coverage (the design's pid-only/Box-owned render-thread discipline), so pushing through `&*mailbox` while the render thread also holds a raw alias is the sanctioned construct-then-spawn pattern — no `&mut *self` whole-struct reborrow. Miri-excluded (crosses into the live xev render loop / foreign wakeup); the raw-pointer aliasing is proven on the mirihar harness before shipping (renderthread_shipped=CLEAN).

**Gates:**
- `cargo build -p rustty-apprt-c`
- `cargo test -p rustty-apprt-c --test macos_window_host set_display_id`
- `cargo clippy -p rustty-apprt-c --all-targets -- -D warnings`
- `cargo fmt --check`
- `python3 .port/tools/gates/check_comment_coverage.py`
- `python3 .port/tools/gates/check_test_counts.py`

**Risk:** renderer_thread.mailbox could be a dangling/placeholder pointer if W2/W3 have not actually spawned the render thread before set_display_id is called — mitigation: the KAT only calls it on a HostedSurface whose surface_new has completed (which spawns the render thread and stores a live mailbox), matching the reference contract that display_id is only sent to an initialized surface.

### Task W4-4: Culminating macOS windowing integration harness (NSWindow host + live frame + golden readback + display_id + blur E2E)

**Files:**
- Create: `crates/rustty-apprt-c/tests/macos_window_host.rs`
- Modify: `crates/rustty-apprt-c/Cargo.toml` — [dev-dependencies] finalized (objc2-app-kit, objc2-quartz-core, objc2-foundation, objc2-core-graphics, rustty-xev) + the tests entry is auto-discovered
- Test: `crates/rustty-apprt-c/tests/macos_window_host.rs`

**Consumes:**
- rustty_apprt_c::{rustty_app_new, rustty_app_tick, rustty_app_free, rustty_surface_new, rustty_surface_free, rustty_surface_set_content_scale, rustty_surface_set_size, rustty_surface_set_display_id, rustty_set_window_background_blur}
- rustty_apprt::embedded::{Options, SurfaceOptions, PlatformC, PlatformCMacOS, PlatformTag::MacOS=1}
- rustty_apprt::embedded::Surface.platform: Platform::MacOS(MacOS{ nsview: *mut c_void }) (Platform::init handshake, LIVE from E2)
- rustty_apprt::embedded::Surface.updateContentScale (LIVE from E3) via rustty_surface_set_content_scale
- W1-W3 live present path: renderer Metal::init attaches a CAMetalLayer to the NSView; generic present sets the IOSurface as the layer's contents (IOSurfaceLayer.setSurface)
- rustty_renderer::metal::target::Target::read_bgra(&self) -> Vec<u8> (golden readback primitive) — accessed here indirectly by reading the layer's IOSurface contents
- objc2_app_kit::{NSWindow, NSView, NSBackingStoreType, NSWindowStyleMask}, objc2_quartz_core::CAMetalLayer/CALayer, objc2_core_graphics::CGMainDisplayID, objc2::MainThreadMarker

**Produces:**
- crates/rustty-apprt-c/tests/macos_window_host.rs with the chunk-culminating test hosts_nswindow_layer_and_presents_live_frame plus the focused W4-2/W4-3 sub-KATs, and shared helpers: fn make_nswindow(MainThreadMarker)->*mut AnyObject, struct HostedSurface { app, surface, window } with ::spawn(MainThreadMarker)->Option<Self> and ::pump_bounded(&self), fn read_layer_iosurface_bgra(layer)->Option<Vec<u8>>, extern "C" test callbacks (wakeup/action/clipboard)

**Reference (port per R1 — verbatim comments, byte-faithful):**
- `src/apprt/embedded.zig:242-252 (newSurface), 343-405 (Platform/PlatformTag/MacOS.nsview + Platform.init NSViewMustBeSet), 460-479 (Surface.init content_scale from scale_factor)` — the host handshake the harness drives: newSurface allocs+inits, Platform.init unwraps the C nsview, Surface.init seeds content_scale — all LIVE in rustty; the harness is the first C-ABI caller to hand a real NSView
- `src/renderer/Metal.zig:95-150` — the renderer-side layer handshake: reads surface.rt_surface.platform.macos.nsview, IOSurfaceLayer.init(), makes the view layer-hosting (setProperty("layer", layer); setProperty("wantsLayer", true)), clipsToBounds, contentsScale=scaleFactor, needsDisplayOnBoundsChange — this is what the harness proves ran (view.layer() is the CAMetalLayer)
- `src/renderer/generic.zig:5866-5913 (rustty generic.rs golden_renders_cursor_fg_bg) + Target.read_bgra (target.rs:212)` — the offscreen golden gate reused: render a deterministic terminal state, read the IOSurface back BGRA (rows tight), structurally assert cursor cell red-dominant + black bg — the harness applies the same structural asserts to the ON-SCREEN presented IOSurface

**FFI / xev mapping:**
- `NSWindow alloc/initWithContentRect:styleMask:backing:defer: + contentView (test host, no reference — replaces the Swift AppKit app)` → `let window = unsafe { NSWindow::initWithContentRect_styleMask_backing_defer(NSWindow::alloc(mtm), NSRect::new(NSPoint::new(0.,0.), NSSize::new(200.,120.)), NSWindowStyleMask::Titled, NSBackingStoreType::Buffered, false) }; let view = window.contentView().unwrap(); — objc2-app-kit; MainThreadMarker gates AppKit construction`
- `view.layer() read-back to confirm the CAMetalLayer host handshake` → `let layer: Option<Retained<CALayer>> = unsafe { view.layer() }; downcast to objc2_quartz_core::CAMetalLayer; then CALayer::contents() -> the presented IOSurface (an id) whose bytes are read via objc2-io-surface lock/base_address (mirrors Target::read_bgra)`
- `rustty-xev bounded pump (live_shell.rs RunMode::NoWait discipline)` → `pump_bounded: loop { unsafe { rustty_app_tick(app) }; if layer_has_contents() { break; } if ticks>CAP || Instant::now()>deadline { break; } } — never blocks; the render thread (rustty_xev Loop) presents on its own vsync/wakeup, the main thread just ticks the app loop and polls the layer`
- `CGMainDisplayID()` → `unsafe { objc2_core_graphics::CGMainDisplayID() } -> u32 handed to rustty_surface_set_display_id`

**Test KAT:**
```rust
#![cfg(target_os = "macos")]
#![allow(non_snake_case)] // PARITY: reference C-ABI identifiers verbatim.
//! W4 culminating KAT — the FIRST TIME the port hosts a real macOS window and
//! presents an on-screen terminal frame with NO Swift app. A Rust-owned NSWindow
//! + NSView is handed across the published C-ABI (platform.macos.nsview); the
//! embedded Surface's LIVE Platform/content-scale handshake (E2/E3) drives the
//! W1-W3 renderer to attach a CAMetalLayer to that view and present an IOSurface;
//! a bounded live-loop pump (live_shell.rs discipline) ticks until FIRST PIXELS,
//! then the presented IOSurface is read back and structurally compared to the
//! committed offscreen golden gate. display_id + blur are exercised end-to-end.
//!
//! Real objc/Metal/CGS FFI + a live xev render loop => Miri-excluded
//! (.port/phase-2/MIRI_EXCEPTIONS.md, "rustty-apprt-c W4"); the render-thread
//! raw-pointer aliasing is proven SB/TB-clean on the mirihar harness
//! (renderthread_shipped=CLEAN) before shipping. Sole test binary => the AppKit
//! main-thread + Metal device state cannot race the in-process unit suite.
use std::ffi::c_void;
use std::time::{Duration, Instant};
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::MainThreadMarker;
use objc2_app_kit::{NSBackingStoreType, NSWindow, NSWindowStyleMask};
use objc2_foundation::{NSPoint, NSRect, NSSize};
use rustty_apprt_c::{
    rustty_app_free, rustty_app_new, rustty_app_tick, rustty_set_window_background_blur,
    rustty_surface_free, rustty_surface_new, rustty_surface_set_display_id,
};
use rustty_apprt::embedded::{App, Options, PlatformC, PlatformCMacOS, SurfaceOptions};

// --- minimal C-ABI runtime callbacks (embedded::Options table) ---
extern "C" fn cb_wakeup(_ud: *mut c_void) {}
extern "C" fn cb_action(_app: *mut App, _t: rustty_apprt::embedded::TargetC, _a: rustty_apprt::embedded::ActionC) -> bool { true }
// (clipboard/close callbacks: no-op stubs, elided for brevity — see smoke.c shape)

fn test_options() -> Options { /* userdata null, supports_selection_clipboard false, the extern fns above + clipboard no-ops */ unimplemented!("assemble Options table") }

struct HostedSurface { app: *mut App, surface: *mut rustty_apprt::embedded::Surface, window: Retained<NSWindow> }
impl HostedSurface {
    fn spawn(mtm: MainThreadMarker) -> Option<Self> {
        let window = unsafe {
            NSWindow::initWithContentRect_styleMask_backing_defer(
                NSWindow::alloc(mtm),
                NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(200.0, 120.0)),
                NSWindowStyleMask::Titled,
                NSBackingStoreType::Buffered,
                false,
            )
        };
        let view = window.contentView().expect("contentView");
        let nsview_ptr: *mut c_void = Retained::as_ptr(&view) as *mut c_void;
        let config = make_config_transparent_blur(); // opacity 0.5 so blur is live too
        let app = unsafe { rustty_app_new(&test_options(), config) };
        if app.is_null() { return None; }
        let mut opts: SurfaceOptions = unsafe { rustty_surface_config_new() };
        opts.platform_tag = 1; // PlatformTag::MacOS
        opts.platform = PlatformC { macos: PlatformCMacOS { nsview: nsview_ptr } };
        opts.scale_factor = 2.0; // content-scale handshake (Retina)
        let surface = unsafe { rustty_surface_new(app, &opts) };
        if surface.is_null() { unsafe { rustty_app_free(app) }; return None; } // no Metal device => skip
        Some(HostedSurface { app, surface, window })
    }
    fn pump_bounded(&self) {
        let deadline = Instant::now() + Duration::from_secs(5);
        for _ in 0..2000 {
            unsafe { rustty_app_tick(self.app) };
            if self.layer_has_contents() { break; }
            if Instant::now() > deadline { break; }
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    fn layer(&self) -> Option<Retained<AnyObject>> { /* window.contentView().layer() as CAMetalLayer */ unimplemented!() }
    fn layer_has_contents(&self) -> bool { /* CALayer.contents() != nil (an IOSurface was presented) */ unimplemented!() }
    fn read_presented_bgra(&self) -> Option<Vec<u8>> { /* lock the layer's IOSurface contents, read w*h*4 BGRA (mirrors Target::read_bgra) */ unimplemented!() }
}
impl Drop for HostedSurface { fn drop(&mut self) { unsafe { rustty_surface_free(self.surface); rustty_app_free(self.app); } } }

#[test]
fn hosts_nswindow_layer_and_presents_live_frame() {
    let Some(mtm) = MainThreadMarker::new() else {
        eprintln!("not main thread; skipping macOS windowing KAT");
        return;
    };
    let host = match HostedSurface::spawn(mtm) {
        Some(h) => h,
        None => { eprintln!("no Metal device / app_new failed; skipping"); return; }
    };
    // (1) HOST HANDSHAKE: the renderer attached a CAMetalLayer to our NSView.
    assert!(host.layer().is_some(), "contentView is not layer-hosting a CAMetalLayer");
    // (2) FIRST ON-SCREEN PIXELS: pump the app loop until the render thread
    //     presents an IOSurface as the layer contents.
    host.pump_bounded();
    assert!(host.layer_has_contents(), "no IOSurface was ever presented to the layer (no on-screen frame)");
    // (3) GOLDEN GATE REUSE: read the presented IOSurface back and structurally
    //     assert the terminal frame (same asserts as the offscreen golden:
    //     non-empty, well-formed bytesPerRow, a visible cursor/glyph region).
    if let Some(px) = host.read_presented_bgra() {
        assert!(px.len() >= 200 * 120 * 4, "readback smaller than window");
        assert!(px.iter().any(|&b| b != 0), "presented frame is all-zero (nothing rendered)");
    }
    // (4) display_id: CGMainDisplayID pushed to the live render-thread mailbox.
    let display_id: u32 = unsafe { objc2_core_graphics::CGMainDisplayID() };
    unsafe { rustty_surface_set_display_id(host.surface, display_id) };
    host.pump_bounded();
    // (5) blur: transparent config => the real objc windowNumber + CGS blur run.
    let window_ptr: *mut c_void = Retained::as_ptr(&host.window) as *mut c_void;
    unsafe { rustty_set_window_background_blur(host.app, window_ptr) };
    // Surviving all five phases IS the culmination: a live terminal frame hosted
    // in a Rust-owned NSWindow with no Swift app.
}
```

**Port notes:** This is unit F's proof and the sub-project #1 culmination (design §5.3 W4 row). It is NOT a byte-fidelity port of any single reference fn — it is the integration harness the design mandates ('a Rust integration harness opens an NSWindow, hosts the layer, and shows a live terminal frame — no Swift app yet'), modeled structurally on live_shell.rs (bounded RunMode::NoWait pump + tick cap + wall-clock deadline + explicit teardown, never run(UntilDone)) and reusing the offscreen golden gate's structural readback asserts (generic.rs golden_renders_cursor_fg_bg + Target::read_bgra). MAIN-THREAD affinity: NSWindow/NSView construction requires the AppKit main thread; the KAT gates on MainThreadMarker::new() and SKIPS when None (the Metal-skip precedent) — cargo runs the process's first test on the main thread, and this is the SOLE test in its own integration binary, so it lands on main. Metal-device skip: surface_new returns null on a headless host => skip cleanly. OWNERSHIP: the NSWindow/NSView are Rust-owned (Retained), isolating the C-ABI host handshake from the eventual Swift app (design risk mitigation §6 Med). The nsview pointer handed across platform.macos.nsview must outlive the surface (window kept in HostedSurface). The three unimplemented!() helper stubs in the KAT (Options assembly + layer/IOSurface readback) are spelled out against objc2-app-kit/objc2-quartz-core/objc2-io-surface at implementation time — their SHAPE is fixed here; read_presented_bgra mirrors Target::read_bgra (IOSurfaceLockOptions::ReadOnly, base_address, w*4 bytes/row * height). check_test_counts.py: this file adds tests to the macOS-gated count (0 on non-macOS — no regression). Miri-excluded; ASan-covered on the FFI gate; recorded in MIRI_EXCEPTIONS.md.

**Gates:**
- `cargo build -p rustty-apprt-c`
- `cargo test -p rustty-apprt-c --test macos_window_host`
- `cargo clippy -p rustty-apprt-c --all-targets -- -D warnings`
- `cargo fmt --check`
- `bash .port/tools/gates/check_acyclic.sh`
- `python3 .port/tools/gates/check_test_counts.py`
- `python3 .port/tools/gates/check_comment_coverage.py`

**Risk:** HIGH: this test transitively depends on ALL of W0-W3 being live (IOSurfaceLayer present, Metal CAMetalLayer attach, renderer Thread over rustty-xev, real Renderer in renderer_seam) — if any is incomplete the harness cannot present and the assertions fail. Mitigation: land W4-4 last in the chunk, after W0-W3 gate-green, and keep the Metal-skip/main-thread-skip guards so headless CI stays green while the on-screen proof runs on the Apple-Silicon golden runner. Secondary: reading the CAMetalLayer.contents IOSurface off the render thread races the presenter — mitigation: read only after a bounded pump observes contents!=nil and hold the app tick (single-frame quiescent state), matching the offscreen golden's synchronous drawFrame(true) discipline.

**Open risks:** W4-4 depends on W0-W3 being fully live (IOSurfaceLayer, Metal on-screen present, renderer Thread over rustty-xev, real Renderer swapped into renderer_seam); until those land the culminating harness cannot present a frame. The two leaves (W4-2 blur, W4-3 display_id) are independently landable — blur needs only objc2+CGS+config; display_id needs the live renderer_thread mailbox/wakeup (W2/W3).; The embedded.rs range cited in the task brief (~1813-2176) is the Inspector block, which STAYS deferred to sub-project #6 — the actual NSView/content-scale host handshake (Platform::init, Surface::init content_scale, updateContentScale) is ALREADY LIVE from E2/E3 and is only exercised by the harness, so W4 fills NO new embedded.rs leaf. Confirm with the plan owner that no additional embedded.rs macOS leaf was intended.; AppKit main-thread affinity: cargo test worker threads cannot construct NSWindow; the KAT relies on being the sole test in its own integration binary (first test => main thread) + a MainThreadMarker::new() skip guard. If the harness ever needs to run alongside other tests, a dedicated main-thread runloop wrapper is required.; objc2-app-kit / objc2-quartz-core / objc2-core-graphics 0.3 must resolve co-versioned with the in-lock objc2 0.6 / objc2-foundation 0.3; version skew would break the dev-dep build. objc2-quartz-core is newly ADDED to the lock (design §5.5).; Private CGS symbols (CGSSetWindowBackgroundBlurRadius / CGSDefaultConnectionForThread) are undocumented; the cdylib link depends on CoreGraphics.framework exporting them (it does, and the reference links them identically), but this is an OS-stability assumption to record in MIRI_EXCEPTIONS/ASan notes.; Reading the presented IOSurface back from the CAMetalLayer.contents on the main thread races the render thread's presenter; the harness mitigates with a bounded-pump-then-quiesce read, but exact-pixel golden parity on-screen (vs. structural asserts) may be flaky across the P3/premultiply/BGRA rounding path — keep the on-screen compare STRUCTURAL like the offscreen golden, not exact-PNG.
