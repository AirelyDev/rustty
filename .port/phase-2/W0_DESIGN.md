# W0 DESIGN — objc2-quartz-core dep + cf_release_thread rewire + IOSurfaceLayer port

**Chunk:** W0 of `docs/superpowers/plans/2026-07-08-macos-windowing-sub1.md`
**Tasks:** T1 (deps + xev wiring + mod line), T2 (cf_release_thread → real rustty-xev), T3 (IOSurfaceLayer CALayer-subclass port).
**Disposition:** READY. No blocker. Every unverified surface flagged in the plan was checked against the actual crate sources; the one material correction (the objc2-quartz-core feature name) is captured below.

> **LOCAL-VERIFICATION CAVEAT (objc2-quartz-core).** `objc2-quartz-core` is **NOT in the workspace `Cargo.lock`** and is **absent from the local registry cache** (`~/.cargo/registry`), so its feature-gate claims below (the `CALayer` feature gating `CALayer`/`kCAGravityTopLeft`/`setContentsGravity`/`contents`/`setContents`; `bounds()`/`contentsScale()` gated by `objc2-core-foundation` returning `objc2_core_foundation::CGRect`/`CGFloat`; `CoreAnimation = []` empty marker) are **read from upstream/docs, NOT verified against a resolved local crate**. The **T3 spike MUST confirm these gates against the actually-resolved 0.3.x source BEFORE locking `features = ["CALayer", "CAMetalLayer", "objc2-core-foundation"]`.** If any gate name is wrong it is a one-line feature add/rename (see design risk #1, which already anticipates adding `"std"`) — a port-time adjustment, not a design fork. Do the spike (T1.a first-compile + the dep-smoke KAT) as the very first port step so a wrong feature name surfaces immediately.

This is the faithful (R1) port design. Reference = read-only oracle at `/Users/gael/projects/ghostty`. Citations use CURRENT line numbers verified this session.

---

## 0. Verification summary (what changed vs the plan's stated assumptions)

| Plan assumption | Verified reality | Action |
|---|---|---|
| "objc2-quartz-core 0.3.1 has NO `CALayer`/`CAGravity` feature; enable `CoreAnimation` + `CAMetalLayer`" | **FALSE (per upstream docs — NOT locally resolved; see caveat above).** There IS a `CALayer` cargo feature. `CoreAnimation = []` is an **empty marker** and pulls in nothing. `CALayer.rs` (the whole module incl. `kCAGravityTopLeft`, `contents`, `setContents`, `contentsGravity`) is gated by `feature = "CALayer"`; `bounds()`/`contentsScale()` are additionally gated by `feature = "objc2-core-foundation"`. **This must be reconfirmed by the T3 spike against the resolved local crate before locking the feature list.** | Enable **`CALayer` + `objc2-core-foundation` + `CAMetalLayer`** (NOT `CoreAnimation`). `objc2-core-foundation` is mandatory (gates the two geometry accessors the resize-discard path calls). `CAMetalLayer` is for W1 (harmless now, pulls objc2-metal which the crate already deps). |
| objc2 version compat | objc2-quartz-core 0.3.1 dep req = `>=0.6.1, <0.8.0`; locked objc2 = **0.6.4**. Compatible. | none |
| CALayer accessors may need `MainThreadMarker` | Verified: `bounds`/`contents`/`setContents`/`contentsScale`/`setContentsScale`/`contentsGravity`/`setContentsGravity`/`needsDisplayOnBoundsChange` take **no MTM**. `layer()`/`init()` also need no MTM (CALayer is not a `MainThreadOnly` class). | none — the subclass uses default `thread_kind` (AnyThread); `alloc()` takes no args. |
| `set_surface` main-thread hop closure | `dispatch2::DispatchQueue::exec_async<F>` requires **`F: Send + FnOnce() + 'static`**. `Retained<IOSurfaceLayerClass>` (a CALayer subclass) is **NOT `Send`**, so it cannot be captured directly. | The faithful map captures a `Send` wrapper newtype carrying the layer's raw `*mut` id + the retained surface — this is EXACTLY the reference's block-context struct `SetSurfaceBlock{ layer: objc.c.id, surface: *IOSurface }` (IOSurfaceLayer.zig:87-90). See §3.4. |
| `contents` pointer identity in T3 KAT | `contents()` returns `Option<Retained<AnyObject>>`; unverified whether CALayer stores the IOSurface object identically or boxes it. | KAT keeps the pointer-identity assert as primary; if it fails at port time, relax to `contents().is_some()` (plan-sanctioned fallback). |

**objc2-io-surface** is resolved at **0.3.2** in this workspace (not 0.3.1). `IOSurfaceRef::width(&self)->usize`/`height(&self)->usize` exist (IOSurfaceRef.rs:426 — the CF-wrapper form). Note there is ALSO an objc2-msg-send `width(&self)->NSInteger` on the same type (generated/objc2.rs:207); the port must pick the `usize` CF-form (or cast) — see §3.4 note.

---

## Task W0.T1 — deps + rustty-xev wiring + `metal.rs` mod line

### T1.a — `crates/rustty-renderer/Cargo.toml`
Add a new stanza inside the existing `[target.'cfg(target_os = "macos")'.dependencies]` block (after the `objc2-core-foundation` stanza that currently ends at **line 172**). Use the **explicit per-feature** style the file already follows:

```toml
# objc2-quartz-core: CALayer + the IOSurfaceLayer subclass (W0.T3) and, for W1,
# CAMetalLayer. Feature gate (per upstream docs; CONFIRM via the T3 spike against
# the resolved local crate BEFORE trusting — this crate is not yet in Cargo.lock):
# the `CALayer` feature (NOT `CoreAnimation`, which is an empty marker) gates
# CALayer + `kCAGravityTopLeft` + contents/setContents/contentsGravity;
# `objc2-core-foundation` gates the bounds()/contentsScale() geometry accessors the
# resize-discard path reads; `CAMetalLayer` is enabled now for W1 (harmless; pulls
# objc2-metal which this crate already deps). If a gate name is wrong it is a
# one-line feature add/rename at port time.
objc2-quartz-core = { version = "0.3", default-features = false, features = [
    "CALayer",
    "CAMetalLayer",
    "objc2-core-foundation",
] }
```

- `default-features = false` matches the crate's per-feature discipline and avoids pulling `std`/`objc2-core-video`/`libc`/every other CA* layer type. (The `std` feature is default-on but not required by our code; omit it to stay minimal — if the build needs `alloc`/`std` glue it will surface as a clear error, then add `"std"`.)
- **No `[workspace.dependencies]` entry** — the objc2-* crates are pinned inline with literal versions (matching `objc2 = "0.6"`, `objc2-metal = "0.3"` already in this file). Confirmed by inspection of the existing macOS block (lines 81-180).

### T1.b — `crates/rustty-os/Cargo.toml`
In `[target.'cfg(target_os = "macos")'.dependencies]` (currently lines 55-59):
1. Add `rustty-xev = { workspace = true }` (workspace dep already declared at root `Cargo.toml:175` `rustty-xev = { path = "crates/rustty-xev" }`).
2. Extend the existing `objc2-core-foundation = "0.3"` (line 58) → `objc2-core-foundation = { version = "0.3", features = ["CFBase", "CFString"] }` so the live KAT can build a `CFString` and read its retain count via `CFGetRetainCount`.

```toml
[target.'cfg(target_os = "macos")'.dependencies]
objc2 = "0.6"
objc2-foundation = "0.3"
# CFBase (CFGetRetainCount/CFRetained) + CFString for the cf_release live KAT.
objc2-core-foundation = { version = "0.3", features = ["CFBase", "CFString"] }
gettext-sys = { path = "../../crates-sys/gettext-sys" }
# The live libxev kqueue Loop/Async backend for cf_release_thread (macOS-gated;
# rustty_xev::Loop/Async are #[cfg(target_os="macos")]). Acyclic: rustty-xev deps
# only { rustty-global, libc } — strictly below rustty-os.
rustty-xev = { workspace = true }
```

**Acyclicity (R4):** `rustty-os → rustty-xev`; rustty-xev's only deps are `rustty-global` + `libc` (verified `crates/rustty-xev/Cargo.toml`). No back-edge to os/termio/core. `check_acyclic.sh` stays green. `objc2-core-foundation` is a dev/KAT need but must be a normal (not dev) dep because the KAT is an integration test in `tests/` and the retain-count call is in the KAT crate, not the lib — actually `CFGetRetainCount`/`CFString` are used only by `tests/cf_release_live.rs`, so they could be a `[dev-dependencies]` feature bump. **Decision:** bump the feature on the existing normal `objc2-core-foundation` dep (simplest, the crate is already a normal dep; a dev-only duplicate stanza would fight Cargo's feature unification). Documented in the stanza comment.

### T1.c — `crates/rustty-renderer/src/metal.rs`
Add the module line so T3's file compiles. Current mod list is lines 34-44 (alphabetical-ish). Insert `pub mod iosurface_layer;` between `pub mod gpu_abi;` (line 37) and `pub mod metal_impl;` (line 38):

```rust
pub mod frame;
pub mod gpu_abi;
pub mod iosurface_layer;   // W0.T3 — CALayer subclass present primitive
pub mod metal_impl;
```

### T1 dep-smoke KAT
Ships at the bottom of `iosurface_layer.rs` (co-located with T3) exactly as the plan gives it (plan lines 131-147). One correction to verify against the crate: `kCAGravityTopLeft` is `pub static kCAGravityTopLeft: &'static CALayerContentsGravity` (an `extern "C"` static that IS a reference), so `unsafe { objc2_quartz_core::kCAGravityTopLeft }` yields `&'static CALayerContentsGravity` and binds to `let _g: &CALayerContentsGravity` directly. `CALayer::class()` resolves via `objc2::ClassType`. Keep the KAT verbatim.

### T1 gates
`cargo build -p rustty-renderer`, `cargo build -p rustty-os`, `cargo tree -p rustty-renderer -i objc2-quartz-core`, `bash .port/tools/gates/check_acyclic.sh`.

---

## Task W0.T2 — rewire `cf_release_thread.rs` onto real rustty-xev

**File:** `crates/rustty-os/src/cf_release_thread.rs` (current full file read; 377 lines). Reference: `ghostty src/os/cf_release_thread.zig` (192 lines, verified).

### T2.1 — module doc (lines 1-32 of the current .rs)
Replace the **DEFERRED-SEAM** parity note (current lines 15-22, the `PARITY (DEFERRED SEAM, R4)` paragraph) with a note stating the seam is now satisfied by the real rustty-xev kqueue backend. Keep the "PORT STATUS: ported", the "Ports the reference's ... every line/comment/test (R1)", and the reference-has-0-tests note. The new note (R5 un-defer):

> PARITY (SEAM SATISFIED, R4): the reference imports `xev = @import("../global.zig").xev`. In rustty that backend is now the real `rustty-xev` kqueue Loop/Async (Milestone A/B), a downward-acyclic macOS edge (rustty-xev deps only `{ rustty-global, libc }`). The full thread logic below is ported verbatim against the live `xev` surface (`Loop`/`Async`/`Completion`/`CallbackAction`/`WaitError=MachPortError`/`RunMode::UntilDone`). No reference logic, comment, or behavior is dropped.

Update the PORT note at current lines 27-32 that says "the `xev` seam (see the module note above)" → "the real `rustty_xev` backend".

### T2.2 — import (current line 42)
Replace `use xev_seam as xev;` with:
```rust
// PORT: `const xev = @import("../global.zig").xev;` — now the real kqueue backend.
use rustty_xev as xev;
```

### T2.3 — DELETE the local placeholder `mod xev_seam { ... }` (current lines 291-376)
Remove the entire `mod xev_seam` block AND the `PARITY (DEFERRED SEAM, R4)` doc-comment that precedes it (current lines 291-300). Its `unimplemented!()` `Loop`/`Async`/`Completion`/`Error`/`AsyncWaitError`/`CallbackAction`/`RunMode` are all replaced by the real re-exports from `rustty_xev`.

**Name reconciliation** (verified against `rustty-xev/src/lib.rs`):

| placeholder name (deleted) | real `rustty_xev` name | source |
|---|---|---|
| `xev::Loop` | `rustty_xev::Loop` | lib.rs:166 (kqueue re-export) |
| `xev::Async` | `rustty_xev::Async` | lib.rs:164 (Copy; `#[derive(Clone,Copy)]` async.rs:109) |
| `xev::Completion` | `rustty_xev::Completion` | lib.rs:166 (`Default`, kqueue.rs:1492) |
| `xev::Error` | `rustty_xev::Error` | lib.rs (`pub struct Error`) |
| `xev::AsyncWaitError` | **`rustty_xev::MachPortError`** | lib.rs:166; `Async::WaitError = MachPortError` (async.rs:90) |
| `xev::CallbackAction::{Rearm,Disarm}` | same (loop.rs:116-132) | lib.rs:172 |
| `xev::RunMode::UntilDone` | same (loop.rs:63-74) | lib.rs:172 |

**IMPORTANT — the callback error-type name changes.** The current .rs callbacks (lines 220,242) spell `r: Result<(), xev::AsyncWaitError>`. The real backend has NO `AsyncWaitError`; it is `xev::MachPortError`. This is the single most-likely compile break of the mechanical rename. Both `wakeupCallback`/`stopCallback` signatures must become `r: core::result::Result<(), xev::MachPortError>` (matching the termio precedent, thread.rs:1123,1153).

### T2.4 — macOS gating
Gate under `#[cfg(target_os = "macos")]`: the `Thread` struct + `impl Thread` + `wakeupCallback` + `stopCallback` + the `Flags` struct + the `log_*` helpers (they name `xev::Error`/`xev::MachPortError`, which are macOS-only in rustty-xev). Keep **cross-platform**: `Message`, `Mailbox` (no xev), and the `CFRelease` extern + the non-macOS `CFRelease` shim (current lines 280-289). On non-macOS the file still type-checks (nothing constructs `Thread`).

Concretely, wrap with `#[cfg(target_os = "macos")]` on:
- `use rustty_xev as xev;`
- `pub struct Thread { ... }` and `pub struct Flags { ... }` and `impl Thread { ... }`
- `fn wakeupCallback(...)`, `fn stopCallback(...)`, all `fn log_*`

`Message`/`Mailbox`/the `#![allow(...)]`/`use rustty_datastruct::BlockingQueue;` stay unguarded.

### T2.5 — the raw-`*mut Thread` callback ABI (CRITICAL — SB/TB discipline)
This is the substantive change beyond the mechanical rename. The current .rs `threadMain_(&mut self)` + `wakeupCallback(self_: Option<&mut Thread>, ...)` uses the OLD placeholder ABI. The real `Async::wait` (async.rs:216) is:

```rust
pub unsafe fn wait<UD, CB>(self, loop_: *mut Loop, c: *mut Completion, userdata: *mut UD, cb: CB)
  where UD: 'static,
        CB: Fn(*mut UD, *mut Loop, *mut Completion, Result<(), WaitError>) -> CallbackAction + 'static;
```

`CB` must be a **zero-sized fn-item** (asserted `size_of::<CB>()==0` in `wait`). `userdata` is a raw `*mut UD` held by the loop across ticks. Per the memory `rust-mut-args-strongly-protected` + `shaper-hook-sb-clean-interleave`: a long-lived `&mut self` whose interior (`loop_`, `wakeup_c`) is simultaneously handed to the loop as raw pointers is **SB/TB UB**. So `threadMain_` must take **raw `*mut Thread`**, mirroring termio thread.rs:734 exactly.

**Method signature changes** (here `UD = Thread`, so `userdata = self_: *mut Thread` and the callbacks take `self_: *mut Thread` directly — NOT a `CallbackData` shuttle, because cf_release's loop userdata IS the Thread, matching the reference `.wait(&self.loop, &self.wakeup_c, Thread, self, wakeupCallback)`):

```rust
// public entrypoint — keep &mut self (the reference `threadMain(self: *Thread)`); it
// delegates to the raw-ptr child so the raw aliasing lives only where the loop holds it.
pub fn threadMain(&mut self) {
    let self_: *mut Thread = self;
    // Call child function so we can use errors...
    if let Err(err) = unsafe { Self::threadMain_(self_) } {
        log_warn_thread_err(&err);
    }
    // If our loop is not stopped ... drain path (unchanged logic):
    if !unsafe { (*self_).loop_.stopped() } {
        log_warn_abrupt_exit();
        unsafe { (*self_).flags.drain = true };
        if let Err(err) = unsafe { (*self_).loop_.run(xev::RunMode::UntilDone) } {
            log_err_drain_failed(&err);
        }
        log_debug_fully_exiting();
    }
}

unsafe fn threadMain_(self_: *mut Thread) -> Result<(), xev::Error> {
    // defer log.debug("cf release thread exited", .{});
    #[cfg(target_os = "macos")]
    unsafe { crate::macos::pthread_setname_np(c"cf_release".as_ptr()) };

    // Start the async handlers. (self.wakeup/self.stop are Copy Asyncs.)
    // self.wakeup.wait(&self.loop, &self.wakeup_c, Thread, self, wakeupCallback);
    unsafe {
        (*self_).wakeup.wait(
            &raw mut (*self_).loop_,
            &raw mut (*self_).wakeup_c,
            self_,
            wakeupCallback,
        );
        (*self_).stop.wait(
            &raw mut (*self_).loop_,
            &raw mut (*self_).stop_c,
            self_,
            stopCallback,
        );
    }

    log_debug_starting();
    let result = unsafe { (*self_).loop_.run(xev::RunMode::UntilDone) };
    log_debug_shutdown();
    result
}
```

Note `(*self_).wakeup` is read out by-copy as the receiver of `.wait(self, ...)` (Async is `Copy`, `wait` takes `self` by value) — this does not create a `&mut` to `wakeup`, so no aliasing with the `&raw mut (*self_).wakeup_c` argument.

`drainMailbox` stays `unsafe fn drainMailbox(self_: *mut Thread)` (raw) — it is only reached from `wakeupCallback` via the raw `self_`; body is the current lines 191-213 with `self.flags`/`self.mailbox` → `(*self_).flags`/`(*self_).mailbox`. Keep the `Message::Release { refs }` arm, the `CFRelease(*r)` loop, the `drop(refs)` (PORT note for `msg.alloc.free`).

**Callbacks** (raw `*mut Thread`, `MachPortError`). This is the **faithful minimal delta** over the existing file (current lines 216-248): ONLY the receiver ABI (`Option<&mut Thread>`→`*mut Thread`, `&mut xev::{Loop,Completion}`→`*mut …`) and the callback error type (`xev::AsyncWaitError`→`xev::MachPortError`) change. **Keep the existing file's `if let Err(err) = r` structure and its `log_err_wakeup(&err)`/`log_err_drain_mailbox(&err)` helper calls verbatim** — do NOT inline them to `if r.is_err()` + comments (that would drop the R1 log-message references the helpers carry):
```rust
fn wakeupCallback(
    self_: *mut Thread,
    _loop: *mut xev::Loop,
    _c: *mut xev::Completion,
    r: core::result::Result<(), xev::MachPortError>,
) -> xev::CallbackAction {
    if let Err(err) = r {
        log_err_wakeup(&err);
        return xev::CallbackAction::Rearm;
    }

    // const t = self_.?;  -> the reference unwraps the optional; our UD is non-null by construction.

    // When we wake up, we check the mailbox. Mailbox producers should
    // wake up our thread after publishing.
    if let Err(err) = unsafe { Thread::drainMailbox(self_) } {
        log_err_drain_mailbox(&err);
    }

    xev::CallbackAction::Rearm
}

fn stopCallback(
    self_: *mut Thread,
    _loop: *mut xev::Loop,
    _c: *mut xev::Completion,
    r: core::result::Result<(), xev::MachPortError>,
) -> xev::CallbackAction {
    // _ = r catch unreachable;
    r.unwrap_or_else(|_| unreachable!());
    unsafe { (*self_).loop_.stop() };
    xev::CallbackAction::Disarm
}
```

`log_err_wakeup`'s parameter type changes only `&xev::AsyncWaitError` → `&xev::MachPortError` (the wakeup callback's error is now `MachPortError`); `log_err_drain_mailbox` stays `&xev::Error` (drainMailbox still returns `Result<(), xev::Error>`). **Keep both helpers and all `log_*` helper comment bodies verbatim (R1)** — the helper bodies are the only carriers of `// log.err("error in wakeup err={}", ...)` / `// log.err("error draining mailbox err={}", ...)`, so removing the calls would regress `check_comment_coverage.py`.

### T2.6 — `deinit`
Reference `deinit(self: *Thread)` is `&mut self` in Zig but here the current .rs has `pub fn deinit(mut self)` (consuming). The KAT calls `t.deinit()` at the end (consuming is fine — the KAT drops `t` after). Keep `pub fn deinit(mut self)`: `self.stop.deinit(); self.wakeup.deinit(); self.loop_.deinit(); self.mailbox.destroy();` — unchanged (Async/Loop `deinit(&mut self)` verified async.rs:184 / kqueue.rs:363).

### T2.7 — field visibility + `init`
- Make `wakeup`, `stop`, `mailbox` **`pub`** (plan Produces; Zig package-visibility; the W1 renderer + this KAT push+notify). `loop_` stays `pub` too per plan Produces (`pub loop_`), but `wakeup_c`/`stop_c`/`flags` stay private.
- `init()` body is unchanged (current lines 104-130) — already calls the real-surface-shaped `xev::Loop::init()?` / `xev::Async::init()?` / `Mailbox::create()` / `xev::Completion::default()`. Under the real backend these now resolve to live kqueue/mach-port syscalls. Keep the `// errdefer` comment markers (R1).
- Re-export: the KAT imports `rustty_os::CFReleaseThread::{Message, Thread}`. Confirm `crates/rustty-os/src/lib.rs` (or `main.rs`) exposes the module as `CFReleaseThread` (PascalCase — the reference file `Thread = @This()` naming). **VERIFY at port time** the existing pub path; if the module is currently `pub mod cf_release_thread;` re-exported as `CFReleaseThread`, keep it; otherwise the KAT's `use` path must match the actual public name. (This is a lib-surface check, not a design fork.)

### T2.8 — the live KAT
`crates/rustty-os/tests/cf_release_live.rs` — ships verbatim as plan lines 206-263. Notes:
- `#![cfg(target_os = "macos")]` + `#![allow(non_snake_case)]`.
- Uses `objc2_core_foundation::{CFGetRetainCount, CFRetained, CFString}` (needs the `CFBase` feature → `CFGetRetainCount`/`CFRetained`; `CFString` feature → `CFString`; both added in T1.b).
- `rustty_datastruct::blocking_queue::Timeout::Instant` — **VERIFIED on the public path**: `crates/rustty-datastruct/src/lib.rs:15` is `pub mod blocking_queue;` and `blocking_queue.rs:22-24` is `pub enum Timeout { Instant, .. }`. No surface change needed.
- **NO-UAF / live-reference invariant (mandatory, per adversary Fix 4).** The retain-count KAT (plan lines 234-262) MUST keep exactly one live owning reference — `kept: CFRetained<CFString>` — throughout, and hand the thread a *separate* `+1` raw ref via `CFRetained::into_raw(kept.clone())`. The thread `CFRelease`s only that extra `+1`, so the count drops N→N−1 (2→1) while `kept` still owns a live `+1`. The post-join assert reads `CFGetRetainCount(Some(kept.as_ref()))` **through the still-live `kept`** — it must NEVER read the raw ref the thread released (that would be a use-after-free once the count could reach 0). `drop(kept)` is the LAST statement (after both the assert and `t.deinit()`), never before the final retain-count read. Do not reorder: reading the retain count of a fully-released object is UB. If the retain-count assert is ever flaky (CFString interning etc.), the sanctioned fallback is the join-succeeds + no-panic proof (the object is never read after release either way).
- The `Async` handles (`t.wakeup`, `t.stop`) copy out (Copy, `u32` mach port ⇒ `Send`) to the notifier thread; `Thread` stays on the test thread (the `Box<Mailbox>` heap addr is stable across the `Thread` value; only the `u32` Asyncs cross threads). This is the SB-clean split.
- **Race note (documented risk):** `wakeup.notify()` may fire before `threadMain` arms the wait; mitigated by the qlimit-1 mach port holding one pending notify (async.rs `notify` design). The 50ms sleep before `stop.notify()` keeps the loop live. Fallback proof if the retain-count assert is flaky: `t.threadMain()` returning + `notifier.join()` succeeding proves the REAL loop ran (the old `unimplemented!()` would have panicked).
- Add `crates/rustty-os/tests/cf_release_live.rs` to `.port/phase-2/MIRI_EXCEPTIONS.md` (real mach/kqueue syscalls + thread spawn; ASan-eligible per R6).

### T2 gates
`cargo build -p rustty-os`, `cargo test -p rustty-os --test cf_release_live -- --nocapture`, `cargo test -p rustty-os --lib`, `cargo clippy -p rustty-os --all-targets`, `check_acyclic.sh`.

---

## Task W0.T3 — port `IOSurfaceLayer.zig` → `metal/iosurface_layer.rs`

**File:** create `crates/rustty-renderer/src/metal/iosurface_layer.rs`. Reference: `ghostty src/renderer/metal/IOSurfaceLayer.zig` (184 lines, fully read).

### T3.1 — module doc + Canonical Interface
Snake_case public API (plan Canonical Interfaces §28-43 wins over the plan-body camelCase in Produces). Public surface:

```rust
pub struct IOSurfaceLayer { pub layer: Retained<IOSurfaceLayerClass> }
pub enum Error { ObjCFailed }                         // ref `error{ObjCFailed}`
pub type DisplayCallback = Option<unsafe extern "C-unwind" fn(*mut c_void)>; // ref ?*align(8) const fn(?*anyopaque) void

impl IOSurfaceLayer {
    pub fn init() -> Result<IOSurfaceLayer, Error>;
    pub fn release(&mut self);                          // Canonical: &mut self (ref `release(self: *IOSurfaceLayer)`)
    pub fn set_surface(&self, surface: &IOSurfaceRef) -> Result<(), Error>;   // ref `!void`
    pub fn set_surface_sync(&self, surface: &IOSurfaceRef);
    pub fn set_display_callback(&self, display_cb: DisplayCallback, display_ctx: *mut c_void);
    pub fn as_ca_layer(&self) -> &objc2_quartz_core::CALayer;  // Canonical upcast for W1 NSView attach
}
```

Module doc ports IOSurfaceLayer.zig:1-2 verbatim ("A wrapper around a CALayer with a utility method for settings its `contents` to an IOSurface."). The Zig `var Subclass: ?objc.Class = null` singleton (lines 14-16) is subsumed by objc2 `define_class!` (registers the pair ONCE at first `IOSurfaceLayerClass::class()`); port lines 14-16 as a comment above the `define_class!` explaining the singleton is now the macro-generated lazy registration.

### T3.2 — the `define_class!` subclass (verified objc2 0.6.4 form)
objc2 0.6.4 `define_class!` supports `#[unsafe(super(CALayer))]`, `#[ivars = Ivars]`, protocol impls, and **plain `impl` method-override blocks** with `#[unsafe(method(selector:))]`. The subclass is **AnyThread** (default thread_kind) → `alloc()` needs no `MainThreadMarker`.

```rust
use std::cell::Cell;
use std::ffi::c_void;
use objc2::rc::{Allocated, Retained};
use objc2::runtime::AnyObject;
use objc2::{define_class, msg_send, DefinedClass};
use objc2_quartz_core::CALayer;

// PORT: `subclass.addIvar("display_cb"); subclass.addIvar("display_ctx")` (lines 148-149).
// objc2 ivars can't hold an Option<fn>, so DisplayCallback is stored as raw bits
// (transmute round-trip) exactly as the reference stores it via setInstanceVariable
// of an objc id built from @intFromPtr(display_cb).
pub struct Ivars {
    display_cb: Cell<*const c_void>,
    display_ctx: Cell<*mut c_void>,
}

define_class!(
    // SAFETY:
    // - Superclass CALayer has no subclassing requirements beyond ObjC.
    // - IOSurfaceLayerClass does not implement Drop.
    #[unsafe(super(CALayer))]
    #[name = "IOSurfaceLayer"]
    #[ivars = Ivars]
    pub struct IOSurfaceLayerClass;

    impl IOSurfaceLayerClass {
        // PORT: subclass.replaceMethod("display", ...) (lines 151-162).
        #[unsafe(method(display))]
        fn display(&self) {
            let bits = self.ivars().display_cb.get();
            // DisplayCallback = Option<unsafe extern "C-unwind" fn(*mut c_void)>
            let cb: DisplayCallback = unsafe { core::mem::transmute(bits) };
            if let Some(cb) = cb {
                unsafe { cb(self.ivars().display_ctx.get()) };
            }
        }

        // PORT: Disable all animations by returning null for all actions (lines 164-176).
        #[unsafe(method(actionForKey:))]
        fn action_for_key(&self, _key: *mut AnyObject) -> *mut AnyObject {
            // return objc.getClass("NSNull").?.msgSend(objc.c.id, "null", .{});
            unsafe { msg_send![objc2::class!(NSNull), null] }
        }
    }
);
```

Notes:
- `#[name = "IOSurfaceLayer"]` registers the ObjC class under the reference's exact name (matches `allocateClassPair(CALayer, "IOSurfaceLayer")`, line 145).
- `display`/`action_for_key` are **method overrides**, not protocol conformances, so they live in a plain `impl` block (objc2 0.6.4 supports this).
- `transmute(*const c_void → Option<fn>)`: sound because a null `*const c_void` transmutes to `None` (niche) and a non-null to `Some(fn)` — this is the Rust analogue of the reference's `@ptrFromInt(@intFromPtr(...))` (lines 155-157). Guard with a `const { assert!(size_of::<*const c_void>() == size_of::<DisplayCallback>()) }` for safety documentation.

### T3.3 — `init` + `release` + `as_ca_layer`
Reference lines 21-44. The reference `[CALayer layer].retain()` (autoreleased +1, then explicit retain → net +1 owned) maps to `alloc().set_ivars(..).init()` (net +1, no autorelease pool) — behavior-equivalent (+1 owned subclass instance). Ivars default to null, matching the reference's explicit post-init `setInstanceVariable(null)` (lines 36-37).

```rust
impl IOSurfaceLayer {
    pub fn init() -> Result<IOSurfaceLayer, Error> {
        // PORT: the reference retains the autoreleased `[CALayer layer]`; alloc+init
        // nets the same +1-owned instance with no autorelease pool (comment lines 22-25).
        let this: Allocated<IOSurfaceLayerClass> = IOSurfaceLayerClass::alloc().set_ivars(Ivars {
            display_cb: Cell::new(core::ptr::null()),
            display_ctx: Cell::new(core::ptr::null_mut()),
        });
        let layer: Retained<IOSurfaceLayerClass> = unsafe { msg_send![super(this), init] };

        // PORT: layer gravity = top-left so contents aren't stretched during resize
        // before a new frame is drawn (comment lines 32-33).
        unsafe { layer.setContentsGravity(objc2_quartz_core::kCAGravityTopLeft) };

        Ok(IOSurfaceLayer { layer })
    }

    // PORT: `release(self: *IOSurfaceLayer) { self.layer.release(); }` (lines 42-44) —
    // ARC drop of the Retained. Canonical Interface: &mut self.
    pub fn release(&mut self) {
        // The Retained<IOSurfaceLayerClass> CFReleases on drop; explicit release is a
        // no-op relative to Drop, so this documents the reference's manual release.
    }

    pub fn as_ca_layer(&self) -> &CALayer {
        // define_class super(CALayer) ⇒ Deref<Target = CALayer>.
        &self.layer
    }
}
```

- `release(&mut self)`: the Canonical Interface mandates `&mut self`. Under ARC the Retained drops when `IOSurfaceLayer` drops; a `release` that force-drops early would need to consume. Since the Canonical signature is `&mut self`, model it as documented-noop + rely on `Drop` (Metal.zig deinit calls `self.layer.release()` once at teardown, which maps to the `IOSurfaceLayer` value dropping — W1-1 wires `layer: Option<IOSurfaceLayer>` whose `None`-set/drop is the release). **Alternative if a hard release is required:** store `layer: Option<Retained<..>>` and `.take()` in release — but the Canonical struct is `layer: Retained<..>` (non-optional), so the Drop model is the faithful fit. Documented divergence.
- `setContentsGravity` takes `&CALayerContentsGravity`; `kCAGravityTopLeft` is `&'static CALayerContentsGravity` — pass directly (no `Some`).

### T3.4 — `set_surface` (main-thread hop, Result) + `set_surface_sync` + callback
Reference lines 46-119. **Key design decision (Send bound):** `dispatch2::DispatchQueue::exec_async<F: Send + FnOnce() + 'static>` cannot capture `Retained<IOSurfaceLayerClass>` (CALayer subclass is not `Send`). The reference's own block context struct `SetSurfaceBlock{ layer: objc.c.id, surface: *IOSurface }` (lines 87-90) passes the layer as a **raw id** (auto-retained on block copy) — so the faithful port captures a `Send` newtype carrying the raw layer id + the retained surface:

```rust
use objc2_core_foundation::{CFRetained, CGFloat, CGRect};
use objc2_io_surface::IOSurfaceRef;

// PORT: `const SetSurfaceBlock = objc.Block(struct { layer: objc.c.id, surface: *IOSurface }, .{}, void);`
// (lines 87-90). The block context. Send is sound: `layer` is a retained CALayer id
// (thread-safe to setProperty:contents on the main queue) and IOSurfaceRef is Send/Sync.
struct SetSurfaceArg {
    layer: *mut IOSurfaceLayerClass,      // objc.c.id — retained +1 for the hop
    surface: CFRetained<IOSurfaceRef>,    // retained; CFReleases when the closure drops (== ref `defer surface.release()`)
}
unsafe impl Send for SetSurfaceArg {}

impl IOSurfaceLayer {
    pub fn set_surface(&self, surface: &IOSurfaceRef) -> Result<(), Error> {
        // PORT: retain the surface so it's not GC'd before we set it; release in the
        // callback after setting contents (comment lines 50-53).
        let owned: CFRetained<IOSurfaceRef> = surface.retain();
        // PORT NOTE lines 55-57: self.layer passed as objc.c.id ⇒ auto-retained on
        // block copy; here we retain it explicitly for the raw id we move into the arg.
        let layer_id: *mut IOSurfaceLayerClass = {
            let r: Retained<IOSurfaceLayerClass> = self.layer.clone(); // +1
            Retained::into_raw(r)
        };
        let arg = SetSurfaceArg { layer: layer_id, surface: owned };

        // if (NSThread.isMainThread) setSurfaceCallback(&block) else dispatch_async(main, &block)
        if objc2::MainThreadMarker::new().is_some() {
            Self::set_surface_callback(arg);
        } else {
            dispatch2::DispatchQueue::main().exec_async(move || Self::set_surface_callback(arg));
        }
        Ok(())
    }

    pub fn set_surface_sync(&self, surface: &IOSurfaceRef) {
        // PORT: self.layer.setProperty("contents", surface); (lines 83-85)
        let obj = (surface as *const IOSurfaceRef).cast::<AnyObject>();
        unsafe { self.layer.setContents(Some(&*obj)) };
    }

    fn set_surface_callback(arg: SetSurfaceArg) {
        // Reconstruct the retained layer id; it drops (CFRelease) at end == block auto-release.
        let layer: Retained<IOSurfaceLayerClass> = unsafe { Retained::from_raw(arg.layer).unwrap() };
        let surface = arg.surface; // drops (CFRelease) at fn end == ref `defer surface.release()`

        // PORT: discard wrong-size surfaces to avoid resize jank (comment lines 101-105).
        let bounds: CGRect = layer.bounds();
        let scale: CGFloat = layer.contentsScale();
        let width = (bounds.size.width * scale) as usize;
        let height = (bounds.size.height * scale) as usize;
        if width != surface.width() || height != surface.height() {
            // PORT (comment, R1): the reference logs the discard here (lines 111-114):
            // log.debug(
            //     "setSurfaceCallback(): surface is wrong size for layer, discarding. surface = {d}x{d}, layer = {d}x{d}",
            //     .{ surface.getWidth(), surface.getHeight(), width, height },
            // );
            return;
        }
        let obj = (&*surface as *const IOSurfaceRef).cast::<AnyObject>();
        unsafe { layer.setContents(Some(&*obj)) };
    }
}
```

Design notes:
- **`Result<(), Error>`:** the reference `setSurface` is `!void` but its body has NO fallible operation (retain/block/dispatch never error). The port returns `Ok(())` unconditionally; the `Result` honors the Canonical Interface + the reference's `!void` shape. `Error::ObjCFailed` is reserved (constructed only by `init`'s class-registration failure path, if `define_class` ever surfaces one — in practice `IOSurfaceLayerClass::class()` is infallible, so `init` also returns `Ok`; keep the `Error` type for the `!void`/`error.ObjCFailed` reference parity).
- **`surface.width()`/`height()`:** IOSurfaceRef exposes both a CF-form `width(&self)->usize` (IOSurfaceRef.rs:426) and an objc2-msg form `width(&self)->NSInteger` (objc2.rs:207). If both features are on, the call may be ambiguous — resolve by the CF free functions `IOSurfaceGetWidth(&surface) as usize` OR fully-qualify. **Port-time check:** prefer the `usize`-returning inherent method; if ambiguous, use `objc2_io_surface::IOSurfaceGetWidth(&surface)`. (The existing `target.rs` builds surfaces but never reads width/height back, so there's no established idiom to copy — this is a new call.)
- **`setContents(Some(&AnyObject))`** and **`bounds()`/`contentsScale()`** are `unsafe`/safe per the crate (contents is `unsafe`, bounds/contentsScale safe); wrap the `unsafe` ones. `bounds`/`contentsScale` require the `objc2-core-foundation` feature (enabled T1.a).
- **SB/TB:** the closure captures only owned values (`SetSurfaceArg` = raw ptr + `CFRetained`); no borrow of `self` crosses the dispatch boundary. Clean.

### T3.5 — `set_display_callback`
Reference lines 121-136:
```rust
pub fn set_display_callback(&self, display_cb: DisplayCallback, display_ctx: *mut c_void) {
    // PORT: self.layer.setInstanceVariable("display_cb"/"display_ctx", ...) (lines 128-135)
    self.layer.ivars().display_cb.set(
        display_cb.map_or(core::ptr::null(), |f| f as *const c_void),
    );
    self.layer.ivars().display_ctx.set(display_ctx);
}
```
`Cell` interior mutability lets this take `&self` (reference receiver is `*IOSurfaceLayer` but only writes ivars). `self.layer.ivars()` reaches the subclass `Ivars` (via `DefinedClass`).

### T3.6 — imports / consumes
```rust
use std::cell::Cell;
use std::ffi::c_void;
use objc2::rc::{Allocated, Retained};
use objc2::runtime::AnyObject;
use objc2::{define_class, msg_send, DefinedClass, MainThreadMarker};
use objc2_quartz_core::{CALayer, CALayerContentsGravity, kCAGravityTopLeft};
use objc2_core_foundation::{CFRetained, CGFloat, CGRect};
use objc2_io_surface::IOSurfaceRef;
```
(`CGRect`/`CGFloat` come from `objc2_core_foundation` in this objc2 generation — verified the CALayer accessors return those types under `feature="objc2-core-foundation"`.)

### T3.7 — the T3 KATs
Ship both plan KATs verbatim (plan lines 326-387):
1. `iosurface_layer_sets_surface_as_contents` — `Metal::init_offscreen(AlphaBlending::Native,64,32)` (skip on `Err(NoMetalDevice)`), `metal.init_target(64,32)` (verified `Metal::init_target(&self, usize, usize) -> Result<Target, target::Error>`, metal_impl.rs:539; `target.surface: CFRetained<IOSurfaceRef>` pub, target.rs:68), `IOSurfaceLayer::init()`, assert `contents()` is `None` fresh, `set_surface_sync(&target.surface)`, assert `contents()` `Some` + **pointer identity** (`Retained::as_ptr == &*target.surface as *const IOSurfaceRef`). **Fallback (documented):** if CALayer boxes the IOSurface, relax to `contents().is_some()`.
   - **KAT naming:** the plan KAT calls `layer.setSurfaceSync(...)` (camelCase). Per Canonical Interfaces the method is `set_surface_sync` — **update the KAT to snake_case** (`layer.set_surface_sync(&target.surface)`); the chunk-culminating-test description (plan line 105) names the behavior, not the identifier. Same for `set_display_callback` in KAT 2.
2. `iosurface_layer_display_callback_fires` — `set_display_callback(Some(on_display), 0xABCD as *mut c_void)`, send `-display` via `msg_send![&*layer.layer, display]`, assert `FIRED`.

Both `#[cfg(all(test, target_os = "macos"))]`. Add the file to `.port/phase-2/MIRI_EXCEPTIONS.md` (objc runtime + Metal; ASan-eligible).

### T3.8 — comment coverage (R1)
Port verbatim as comments: the init autorelease/retain rationale (lines 22-25), the block-copy auto-retain note (lines 55-57), the wrong-size-discard jank explanation (lines 101-105), **the discard `log.debug` message string itself (lines 111-114: `"setSurfaceCallback(): surface is wrong size for layer, discarding. surface = {d}x{d}, layer = {d}x{d}"` with its `.{ surface.getWidth(), surface.getHeight(), width, height }` args — ported as a comment in `set_surface_callback`, per §3.4; the existing cf_release port preserves EVERY `log.*` message as a comment, so omitting this one regresses `check_comment_coverage.py`/R1)**, the disable-animations `actionForKey:` comment (line 164), the `DisplayCallback` doc, and the two method docs (`setSurface`/`setSurfaceSync` "Makes sure to do so on the main thread ..." lines 46-48/80-82). `check_comment_coverage.py` must stay green.

### T3 gates
`cargo build -p rustty-renderer`, `cargo test -p rustty-renderer metal::iosurface_layer -- --nocapture`, `cargo clippy -p rustty-renderer --all-targets`, `check_acyclic.sh`, `check_comment_coverage.py`.

---

## 4. Cross-task risks & mitigations

1. **objc2-quartz-core `std` feature:** enabled by default; we set `default-features=false`. If the crate needs `alloc`/`std` glue and errors, add `"std"` to the feature list (low risk, one-line).
2. **`define_class!` plain-method-override block:** verified supported in objc2 0.6.4 (examples use protocol impls; the macro also accepts plain `impl` blocks with `#[unsafe(method(...))]`). If the exact `impl IOSurfaceLayerClass { ... }` form is rejected, the fallback is the manual `ClassBuilder` path (objc2 `class_with_lifetime.rs` example) — same registered class, more boilerplate. Spike this FIRST (highest-uncertainty code).
3. **transmute `*const c_void ↔ Option<fn>`:** relies on fn-pointer niche = null. Sound for `Option<unsafe extern "C-unwind" fn(...)>`. Guard with a `const` size assert.
4. **`contents` pointer-identity assert:** may fail if CALayer boxes the IOSurface → relax to `is_some()` (plan-sanctioned).
5. **cf_release notify/wait arm race:** mitigated by qlimit-1 mach port; join-succeeds is the fallback proof.
6. **`rustty_os::CFReleaseThread` re-export path** and **`rustty_datastruct::blocking_queue::Timeout::Instant`** path: both are surface checks at port time (grep lib.rs), not design forks.
7. **`width()`/`height()` method ambiguity** on IOSurfaceRef (two inherent methods under two features): fully-qualify or use `IOSurfaceGetWidth`/`IOSurfaceGetHeight` free fns.

## 5. Standing gates (all three tasks)
`cargo build --workspace`, `cargo test` (rustty-os / rustty-renderer), `cargo clippy --workspace -- -D warnings`, `cargo fmt --check`, `check_acyclic.sh`, `check_test_counts.py`, `check_comment_coverage.py`. macOS-only tests gated `#[cfg(target_os="macos")]`. Both new tests → `.port/phase-2/MIRI_EXCEPTIONS.md`.
</content>
</invoke>
