# W5 DESIGN — the last live-windowing seam: `Metal::loop_enter` + the display-callback trampoline

Chunk **W5** fills the **single** remaining seam for a live windowed terminal:
`renderer::metal::metal_impl::Metal::loop_enter` (`metal_impl.rs:476`,
`unimplemented!("windowing-host")`), which today aborts any live surface's render
thread. It installs the `IOSurfaceLayer` display callback so Core Animation can
drive a frame, exactly mirroring `ghostty/src/renderer/Metal.zig:164-176`
(`loopEnter` + `displayCallback`).

**Disposition: `ready`** (revised after adversary review — the three blocking
aliasing issues are resolved by a **raw-self draw chain**, below). Every consumed
seam is live: W0 ported `IOSurfaceLayer::set_display_callback` + `DisplayCallback` +
the overridden `display` method; the whole `draw_frame` body is ported and already
routes **all** work through a raw `self_ptr: *mut Renderer`; the container_of /
`@fieldParentPtr` idiom has a live precedent in this very file
(`metal_buffer_completed`, proven on `mirihar-chunk3b`).

**ADVERSARY RESOLUTION (the load-bearing correction).** The prior draft's soundness
premise — "no whole-`Renderer` `&mut` protector ever spans the call" — was **false as
written**: it never modified the two *real* production draw callers, both of which
still went through the `&mut self` wrapper and thus established a protected
whole-`Renderer` `&mut` concurrent with the display callback's raw draw:

- the **render thread** — `renderer_seam.rs:1208`
  `(*(*self_).renderer).0.draw_frame(false)` (macOS: `MUST_DRAW_FROM_APP_THREAD =
  false`, `renderer_seam.rs:84`; ref `Thread.zig:514`), and
- the **main-thread present** — `Surface::draw` (`surface.rs:2795`) →
  `renderer_seam.rs:280` `self.0.draw_frame(sync)`.

Both are now **eliminated**: the *entire draw call chain* is made **raw-self**
(`*mut` receivers), faithful to Zig, where **every** receiver on this chain is a raw
pointer — `drawFrame(self: *Renderer)`, `renderer.drawFrame(self: *Self)`,
`Surface.draw(self: *Surface)`. So the port's `&mut self` on these was itself the
divergence; raw-self is *more* faithful (doctrine `rust-mut-args-strongly-protected`).
The `&mut self` `draw_frame` wrapper is **restricted to the single-threaded offscreen
unit tests** and is no longer reachable from any concurrent path (§2.3, §2.5). The
concurrency soundness is pinned by a corrected `mirihar-chunk3b` candidate that models
the **shipped** shape (both concurrent callers raw-self) as the CLEAN case and the
**rejected** shape (a render-thread `&mut` receiver concurrent with a raw display
callback) as the UB control (§6). `needs_mirihar = true`.

> **AUTHORITY / reference (read in full):** `ghostty/src/renderer/Metal.zig:100-176`
> (the NSView→layer attach, `needsDisplayOnBoundsChange`, `loopEnter`,
> `displayCallback`, `present`), `src/renderer/generic.zig:99-100, 897-916, 1017-1024`
> (the `draw_mutex` doc, `loopEnter`, `hasVsync`), `src/renderer/Thread.zig:1-30,
> 498-630` (the render-thread draw paths + `must_draw_from_app_thread`),
> `src/renderer/metal/IOSurfaceLayer.zig:121-162` (`DisplayCallback`,
> `setDisplayCallback`, the overridden `display`).

---

## 0. Files & current state (verified line numbers, 2026-07-09)

- `crates/rustty-renderer/src/metal/metal_impl.rs`
  - `Metal::loop_enter(&mut self)` — the seam to fill, `:476-480`
    (`unimplemented!("windowing-host")`); the verbatim Zig body of
    `loopEnter`/`displayCallback` is preserved in comments `:461-475`.
  - `pub struct Metal` `:104-…`; `pub layer: Option<IOSurfaceLayer>` `:111`
    (sanctioned-divergence `Option`: `None` == offscreen, `Some` == on-screen).
  - `Metal::init(view, scale_factor, blending)` `:372` — the on-screen ctor that
    builds `Some(layer)` and sets `needsDisplayOnBoundsChange = true` (`:355-356`
    in the verbatim body). Live since W1.
  - `Metal::surface_size` `:691` — `Some(layer)` ⇒ `layer.bounds * contentsScale`;
    `None` ⇒ `offscreen_size`. (Load-bearing for the KAT sizing, §5.)
  - `metal_buffer_completed` **is in `generic.rs`**, not here (see below).
- `crates/rustty-renderer/src/generic.rs`
  - `pub struct Renderer` `:1168`; field **`pub api: Metal` `:1278`** — container_of
    target confirmed (`offset_of!(Renderer, api)`). `pub draw_mutex: Mutex` `:1179`
    (doc `:1175-1176`: "held whenever any state used in `drawFrame` is being modified,
    and also when it's being accessed in `drawFrame`").
  - `Renderer::loop_enter(&mut self, draw_now: Async)` `:2232-2247` — already ported;
    calls `self.api.loop_enter()` `:2238` (the W5 leaf), then the display-link wiring.
  - **`draw_frame(&mut self, sync)` `:5323`** — line 1 is `let self_ptr: *mut Renderer
    = self;` `:5338`; **the entire body already reads/writes only through `self_ptr`**
    (never `self` again). It locks `draw_mutex` at `:5339` and unlocks via the
    `UnlockGuard` RAII (`:5348-5354, 5403`).
  - `metal_buffer_completed` `:5103-5133` — the completion FFI trampoline: recovers
    `ctx.renderer as *mut Renderer` (container_of), derefs it as a **raw** `*mut
    Renderer` (never `&mut`), runs inside `draw_frame` while `self_ptr` is live.
    **PORT NOTE `:5095-5102`: "A `&mut` reborrow held across this callback would be
    UB. … PROVEN clean under Stacked Borrows AND Tree Borrows on
    `.port/phase-2/mirihar-chunk3b` (`drawframe_shipped`)."** This is the exact shape
    W5 extends to the cross-thread case.
  - `frame_completed(&self, health)` `:5283` — takes **`&self`** (shared).
  - The offscreen test harness: `golden_renderer(w,h)` + `draw_frame_bounded_pump`
    `:6407` + `golden_renders_cursor_fg_bg` `:6421` — all call `r.draw_frame(true)`
    single-threaded.
- `crates/rustty-renderer/src/metal/iosurface_layer.rs` — W0, complete:
  - `pub type DisplayCallback = Option<unsafe extern "C-unwind" fn(*mut c_void)>` `:39`.
  - `set_display_callback(&self, cb: DisplayCallback, ctx: *mut c_void)` `:231-239` —
    stores the fn as raw bits and **`ctx` in `Cell<*mut c_void>` (ptr-in-ptr, so
    provenance is preserved across the store/read, NOT an integer round-trip)**.
  - the overridden `display` method `:62-81` reads the ivars and calls
    `cb(display_ctx.get())` — the live fire path; unit-tested by
    `iosurface_layer_display_callback_fires` `:296`.

- `crates/rustty-core/src/renderer_seam.rs` — **now in the change-set (§3.4)**:
  - `Thread::drawFrame(self_: *mut Thread<A>, now)` `:1183` — the render-thread draw
    trigger; line `:1208` `(*(*self_).renderer).0.draw_frame(false)` is the concurrent
    render-thread caller. `MUST_DRAW_FROM_APP_THREAD = false` `:84` (macOS never hops
    to the app thread). Its own callers (`:987, :1270, :1293, :1351`) are unaffected
    (they call `Thread::drawFrame`, already raw-self).
  - `Renderer::drawFrame(&mut self, sync)` `:279-281` — the core-newtype forwarder
    (`Renderer(pub rustty_renderer::Renderer)` `:199`); reached **only** from
    `Surface::draw` (`surface.rs:2795`).
- `crates/rustty-core/src/surface.rs` — **now in the change-set (§3.5)**:
  - `Surface::draw(&mut self)` `:2791-2797` — the main-thread forced-present path
    (`self.renderer.drawFrame(true)` `:2795`). Zig receiver is `self: *Surface` `:2787`.
    Surface embeds `renderer: Renderer` **by value** `:325`; the render thread holds a
    raw `&raw mut (*self_).renderer` into the *same* Surface allocation `:1861` — so a
    live `&mut Surface` protector covers the render thread's `renderer.0` writes.
  - Callers of `Surface::draw`: `embedded.rs:1395`, `apprt-c/src/lib.rs:1154`.
- `crates/rustty-apprt/src/embedded.rs` — **now in the change-set (§3.5)**:
  - `Surface::draw(&mut self)` `:1390-1398` — embeds `core_surface: CoreSurface` **by
    value** `:889`; Zig receiver `self: *Surface` `:1385`. Its **sole** caller is the
    raw C-ABI `rustty_surface_draw` (`apprt-c/src/lib.rs:1154`), so the raw-self cascade
    terminates cleanly at the C boundary.
- `crates/rustty-apprt-c/src/lib.rs` — **now in the change-set (§3.5)**:
  - `rustty_surface_draw(surface: *mut Surface)` `:1151-1154` — already a raw `*mut`;
    `(*surface).draw()` becomes `Surface::draw(surface)`.

**No `#[ignore]`d "W4-4 finale" test exists.** The only ignored renderer test is the
on-device live-`CVDisplayLink` KAT (`cvdisplaylink.rs:319`), which stays ignored.
W5 adds a NEW headless KAT (§5); nothing gets un-ignored (§7).

---

## 1. The seam — reference vs. port shape

Reference (`Metal.zig:164-176`, verbatim in `metal_impl.rs:461-475`):

```zig
pub fn loopEnter(self: *Metal) void {
    const renderer: *align(1) Renderer = @fieldParentPtr("api", self);   // container_of
    self.layer.setDisplayCallback(@ptrCast(&displayCallback), @ptrCast(renderer));
}
fn displayCallback(renderer: *Renderer) align(8) void {
    renderer.drawFrame(true) catch |err|
        log.warn("Error drawing frame in display callback, err={}", .{err});
}
```

Two facts that fix the port shape:

1. **`needsDisplayOnBoundsChange = true`** (`Metal.zig:146`, comment: *"This makes it
   so that our display callback will actually be called."*). Core Animation calls the
   layer's `display` method **on the main thread**, during the `CATransaction` commit,
   whenever the layer is marked needs-display (e.g. a live window resize changes the
   bounds). `display` → the stored `display_cb(display_ctx)` → `renderer.drawFrame(true)`.
2. The Zig receiver is a raw `*Renderer` (and the Thread owns `renderer: *Renderer`,
   also raw). Zig has **no** aliasing model — two raw `*Renderer` calling a
   `*Renderer`-receiver method concurrently is fine as long as `draw_mutex` serialises
   the state access. **This is the entire reason `draw_mutex` exists.**

---

## 2. THE CRUX — is the raw-`*mut Renderer` `draw_frame` call sound? (aliasing_model)

### 2.1 Is the display callback ever concurrent with the render thread's `drawFrame`?

**Yes, on macOS.** Evidence:

- `Thread.zig:26-30` — `must_draw_from_app_thread` defaults to **false** and is
  declared `true` **only** for GTK (`apprt/gtk/App.zig:23`). macOS/AppKit does **not**
  declare it. So on macOS the render thread calls `self.renderer.drawFrame(false)`
  **directly on the render thread** (`Thread.zig:514`), NOT via an app-thread hop.
- The display callback fires on the **main thread** (CA commit, driven by
  `needsDisplayOnBoundsChange` during a resize) → `renderer.drawFrame(true)`.
- Therefore two threads — render thread (`drawFrame(false)` from its CVDisplayLink /
  timer path) and main thread (`drawFrame(true)` from the layer `display`) — can be
  **inside `drawFrame` simultaneously**. `generic.zig:99-100` says exactly this:
  `draw_mutex` "must be held whenever any state used in `drawFrame` is being modified,
  and also when it's being accessed in `drawFrame`."

So the concurrency is real and **the `draw_mutex` is the serialisation mechanism for
the DATA**.

### 2.2 Why the mutex alone does NOT make an `&mut self` re-entry sound in Rust

`draw_frame(&mut self, sync)` takes a **unique reference receiver**. Under Tree
Borrows (and Stacked Borrows), a `&mut` **function-argument** is *protected for the
entire duration of the call* — regardless of the fact that the body immediately
casts it to `self_ptr` and never touches `self` again. If the display callback did
`unsafe { (*renderer).draw_frame(true) }` while the render thread is inside its own
`draw_frame(&mut self)`, **two protected `&mut Renderer` to the same allocation exist
concurrently.** The `draw_mutex` is acquired *inside* the body, *after* both `&mut`
protectors are already established — a foreign retag/read of a protected `&mut` is
**immediate UB**, mutex or no mutex. So a whole-`Renderer` `&mut` reborrow through the
raw ctx is **UNSOUND**.

This is precisely the hazard the existing `metal_buffer_completed` PORT NOTE calls out
(`generic.rs:5095-5102`) for the *same-thread nested* case; W5's display callback is
the *cross-thread concurrent* generalisation of it.

### 2.3 The resolution — a raw-self `draw_frame_raw` entry (faithful to Zig `*Renderer`)

Because `draw_frame`'s body **already** operates exclusively through `self_ptr`
(`:5338` onward), the fix is surgical: promote the raw pointer from a local to the
**receiver**, so no whole-`Renderer` `&mut` protector ever spans the call. This is the
*most* faithful translation of Zig's `drawFrame(self: *Renderer)` (a raw pointer
receiver), and it matches the codebase doctrine `rust-mut-args-strongly-protected`
("use raw-ptr args; Zig ptrs are unprotected").

```rust
// generic.rs — REPLACES the first two lines of the current draw_frame; body UNCHANGED.

/// Raw-self entry. Faithful to Zig `drawFrame(self: *Renderer)` (a raw-pointer
/// receiver — no protector). BOTH concurrent callers invoke THIS through a raw
/// `*mut Renderer`: (a) the render thread's `renderer.Thread` owner (above
/// rustty-renderer; it holds `renderer: *mut Renderer`, faithful to Zig's raw
/// `*Renderer` field), and (b) the main-thread display callback (§3). No
/// whole-`Renderer` `&mut` protector ever spans the call, so the two concurrent
/// calls never alias a protected reference; the `draw_mutex` (locked below)
/// serialises the field accesses. See W5_DESIGN §2. Proven on mirihar-chunk3b
/// (`displaycb_concurrent`).
///
/// # Safety
/// `self_ptr` must be a valid, well-aligned, whole-`Renderer`-provenance pointer,
/// and outlive the call. Concurrent calls are permitted (serialised by `draw_mutex`).
pub unsafe fn draw_frame_raw(self_ptr: *mut Renderer, sync: bool) -> Result<(), Error> {
    // self.draw_mutex.lock();  /  defer self.draw_mutex.unlock();
    unsafe { (*self_ptr).draw_mutex.lock() };
    // ... EVERY remaining line of the current draw_frame body, VERBATIM ...
    // (it already reads/writes only `(*self_ptr).<field>` and the RAII guards hold
    //  `self_ptr`; nothing changes below this point.)
}

/// `&mut self` convenience wrapper. Sound **only** for EXCLUSIVE single-threaded
/// callers — i.e. the offscreen unit tests, where no display link / render thread /
/// CA `display` can fire concurrently. **Every concurrent production path uses
/// `draw_frame_raw`; no `&mut self` draw entry is reachable from the render thread
/// or the main-thread present path** (§2.5 enumerates and re-routes both). Adding a
/// concurrent caller of THIS wrapper reintroduces the protected-`&mut` UB the split
/// exists to prevent — the `displaycb_naive` mirihar control (§6) fails for exactly
/// that shape.
pub fn draw_frame(&mut self, sync: bool) -> Result<(), Error> {
    let self_ptr: *mut Renderer = self;
    unsafe { Self::draw_frame_raw(self_ptr, sync) }
}
```

**Why `draw_frame_raw` is sound under concurrency (the aliasing_model):**

- Entry, *before* the lock: the only access is `(*self_ptr).draw_mutex.lock()`, which
  forms `&(*self_ptr).draw_mutex` — a **shared** borrow of a `Sync` field. Two threads
  concurrently shared-borrowing the same `Mutex` field is fine (shared aliasing of an
  interior-mutable/`Sync` location is permitted under SB/TB). No `&mut` yet.
- After the lock: the body forms only **transient, per-field** `&mut`/`&`
  (`(*self_ptr).api.draw_frame_start()`, `(*self_ptr).cells_rebuilt = …`, the
  `metal_buffer_completed` re-entry, …). Every one of these is created *and dropped*
  while the current thread **holds the `draw_mutex`**, so the other thread is blocked
  in `lock()` (only shared-borrowing the mutex) and forms **no** conflicting reference.
  No two field-`&mut` ever coexist across threads.
- No whole-`Renderer` `&mut` protector ever exists. → No cross-thread protected-`&mut`
  aliasing. Sound under SB **and** TB.

The `&mut self` wrapper is retained purely so the ~dozen offscreen tests keep calling
`r.draw_frame(true)` unchanged; it is single-threaded-exclusive and therefore sound in
that context (a single `&mut self` → its own `self_ptr` child, no foreign access).

### 2.4 Provenance of the recovered `*mut Renderer` (container_of)

`@fieldParentPtr("api", self)` maps to a byte-offset subtraction. For the recovered
`*mut Renderer` to legally deref **all** of `Renderer` (which `draw_frame_raw` does),
it must carry **whole-`Renderer` provenance** — a pointer derived from a *narrowed*
`&mut self.api` (provenance ⊆ the `api` field) would be out-of-range UB. So
`Renderer::loop_enter` hands the leaf a raw `*mut Metal` **derived from the
whole-`Renderer` `self_ptr`** (not a narrowing `&mut self.api` reborrow), and the leaf
recovers the base with `byte_sub`. The round-trip

```
self_ptr:*mut Renderer  →  &raw mut (*self_ptr).api  →  byte_sub(offset_of!(Renderer,api))  →  *mut Renderer
```

is a `+offset` then `−offset` on one unbroken provenance, and the ctx is stored/loaded
as a **pointer** in `Cell<*mut c_void>` (no int round-trip → provenance preserved).
Per doctrine this raw round-trip is **proven, not hand-reasoned** — see §6.

### 2.5 The TWO concurrent production draw callers — both re-routed to raw-self

The display callback (`draw_frame_raw(true)`, main thread) is **not** the only draw
entry. On macOS **three** draw entries exist; the render thread is the one that races
the two main-thread entries (which are mutually exclusive — both on the main runloop):

| # | entry | thread | prior receiver | after §3 |
|---|-------|--------|----------------|----------|
| 1 | render thread — `Thread::drawFrame` `renderer_seam.rs:1208` | render | `(*(*self_).renderer).0.draw_frame(false)` → protected `&mut Renderer` | `draw_frame_raw(&raw mut (*(*self_).renderer).0, false)` — raw |
| 2 | main present — `Surface::draw` `surface.rs:2795` → `renderer_seam.rs:280` | main | `self.0.draw_frame(sync)` → protected `&mut Renderer` | raw-self chain → `draw_frame_raw` |
| 3 | display callback — `display_callback` (§3.2) | main | *(new in W5)* raw | `draw_frame_raw(true)` — raw |

**Caller 1 (render thread) — the adversary's primary counterexample.** Because
`MUST_DRAW_FROM_APP_THREAD = false` (`renderer_seam.rs:84`; ref `Thread.zig:514`), the
render thread calls `drawFrame(false)` **directly on the render thread**. The old
`(*(*self_).renderer).0.draw_frame(false)` reborrows a protected whole-`Renderer`
`&mut` for the entire frame; concurrently the display callback (caller 3) locks
`draw_mutex` and writes a **plain** field — e.g. `cells_rebuilt` (`generic.rs:1231`,
written at `:5444`, not interior-mutable). Under BOTH SB and TB, a foreign write to a
location under a live protected `&mut` (Reserved→Disabled of a protected tag) is
**immediate UB**, and the `draw_mutex` cannot rescue it — the protector is established
at `draw_frame(&mut self)` entry, *before* the body ever reaches `lock()`. The render
thread already holds `self_`, `(*self_).renderer` as raw `*mut`; forming `&raw mut
(*(*self_).renderer).0` is a raw place projection (no reborrow, no protector), so the
render thread now establishes **no** `&mut` at all. ✔

**Caller 2 (main present).** `Surface::draw` blocks in `draw_frame`'s `draw_mutex.lock()`
while holding a live protected reference that covers the render thread's `renderer.0`
writes — the symmetric UB. Naively rewriting *only* the `:280` body to
`draw_frame_raw(&raw mut self.0, sync)` is **cosmetic**: the `&mut self` *receiver* of
the `:280` forwarder is a protected `&mut` over the same bytes as `rustty_renderer::
Renderer` (newtype at offset 0), and one level up `Surface::draw(&mut self)`'s `&mut
Surface` protector covers `self.renderer.0` too (renderer embedded by value,
`surface.rs:325`; the render thread aliases the same allocation, `:1861`). So the fix
must remove the protected `&mut` at **every** level down to the raw entry. It does,
faithfully: **every Zig receiver on this chain is a raw pointer**, so raw-self is the
*correct* port, not a new divergence —

```
rustty_surface_draw(surface: *mut Surface)          // C ABI — already raw
  → apprt::Surface::draw(self_: *mut Surface)        // Zig: draw(self: *Surface)
    → core::Surface::draw(self_: *mut Surface<A>)     // Zig: draw(self: *Surface)
      → core::Renderer::drawFrame(self_: *mut Renderer)  // Zig: drawFrame(self: *Self)
        → rustty_renderer::Renderer::draw_frame_raw(self_ptr, true)  // Zig: drawFrame(self: *Renderer)
```

Each link is `&raw mut (*parent).field` (a raw place projection off a raw `*mut`) — no
`&mut` retag, no protector — so **no protected reference over the `Renderer` bytes
spans caller 2's draw**. The cascade **terminates** at the raw C-ABI entry
(`rustty_surface_draw`), whose sole downstream is `Surface::draw` (verified sole caller
of embedded `draw`, and embedded `draw`/`refresh` were the only main-thread present
reachers). ✔

**Scope boundary (honest — a SEPARATE, pre-existing tree-wide item, NOT a W5 blocker).**
W5 is the first chunk to make the render thread *run* (pre-W5 `loop_enter` is
`unimplemented!()` and aborts the render thread), so it *surfaces* the general fact
that **any** main-thread `&mut self` `Surface`/`Renderer` method concurrent with the
live render thread's plain-field writes is a protected-`&mut` hazard under the port's
`&mut self`-for-`*Self` ergonomic divergence (governed by
`rust-mut-args-strongly-protected`). W5 discharges this **for the draw path** (the
seam it fills, and the only path the adversary flagged) via the raw-self chain above;
converting the remaining concurrently-reachable `Surface` methods
(`keyCallback`/`sizeCallback`/…, each Zig `self: *Surface`) to raw-self is a distinct
port-wide sweep, out of scope here. W5's own deliverable — the draw seam — is sound in
isolation, so W5 is not blocked on that sweep.

---

## 3. Exact port code

### 3.1 `Renderer::loop_enter` (generic.rs:2232) — hand the leaf a whole-`Renderer` ptr

Only the `self.api.loop_enter()` line (`:2238`) changes; the display-link wiring below
it is untouched.

```rust
pub fn loop_enter(&mut self, draw_now: Async) -> Result<(), Error> {
    // Metal DECLARES loopEnter ⇒ install the IOSurfaceLayer display callback.
    // container_of needs WHOLE-Renderer provenance (draw_frame_raw derefs all of
    // Renderer), so hand the leaf a raw *mut Metal derived from the whole-Renderer
    // self_ptr — NOT a narrowing `&mut self.api` reborrow. See W5_DESIGN §2.4.
    let self_ptr: *mut Renderer = self;
    let api_ptr: *mut Metal = unsafe { &raw mut (*self_ptr).api };
    unsafe { Metal::loop_enter(api_ptr) };

    // `if (comptime DisplayLink == void) return;` — macOS: never void.
    let Some(display_link) = self.display_link.as_mut() else { return Ok(()) };
    display_link.set_output_callback(draw_now)?;
    let _ = display_link.start(); // catch {}
    Ok(())
}
```

### 3.2 `Metal::loop_enter` + `display_callback` (metal_impl.rs:476) — the seam

Replace the `unimplemented!` with the faithful `loopEnter`/`displayCallback` pair.
`metal_impl` imports `Renderer` for the `offset_of!` (intra-crate back-reference to
the type it is monomorphised under — allowed; `check_acyclic.sh` is crate-level, and
`Renderer` and `Metal` share the `rustty-renderer` crate).

```rust
use crate::generic::Renderer;                     // add to the module imports
use core::ffi::c_void;

// pub fn loopEnter(self: *Metal) void {
//     const renderer: *align(1) Renderer = @fieldParentPtr("api", self);
//     self.layer.setDisplayCallback(@ptrCast(&displayCallback), @ptrCast(renderer));
// }
//
// # Safety
// `self_ptr` must be a whole-`Renderer`-provenance `*mut Metal` pointing at the
// `Renderer.api` field (established by `Renderer::loop_enter`, §3.1).
pub unsafe fn loop_enter(self_ptr: *mut Metal) {
    // container_of: Metal is Renderer.api (faithful @fieldParentPtr). byte_sub keeps
    // the whole-Renderer provenance of `self_ptr` (§2.4).
    let renderer: *mut Renderer = unsafe {
        (self_ptr as *mut u8)
            .byte_sub(core::mem::offset_of!(Renderer, api))
            .cast::<Renderer>()
    };
    // OFFSCREEN divergence: `layer` is Option. On-screen (loopEnter's only real
    // caller) it is always Some; a defensive `if let` keeps the offscreen ctor safe.
    if let Some(layer) = unsafe { (*self_ptr).layer.as_ref() } {
        // self.layer.setDisplayCallback(&displayCallback, renderer);
        layer.set_display_callback(Some(display_callback), renderer as *mut c_void);
    }
}

// fn displayCallback(renderer: *Renderer) align(8) void {
//     renderer.drawFrame(true) catch |err|
//         log.warn("Error drawing frame in display callback, err={}", .{err});
// }
//
// The ObjC `display` method (main thread, CA commit) fires this. Recovers the raw
// *mut Renderer ctx and calls the RAW-self draw path — NEVER `(*renderer)
// .draw_frame(true)`, which would form a protected whole-Renderer `&mut` and race
// the render thread's own draw (W5_DESIGN §2.2/§2.3).
unsafe extern "C-unwind" fn display_callback(ctx: *mut c_void) {
    let renderer = ctx.cast::<Renderer>();
    if let Err(_err) = unsafe { Renderer::draw_frame_raw(renderer, true) } {
        // log.warn("Error drawing frame in display callback, err={}", .{_err});
        // (no logging facade wired in the port yet — tree-wide `log.*`-as-comment.)
    }
}
```

**Signature change to `Metal::loop_enter`** (`&mut self` → `*mut Metal`): required for
the provenance argument in §2.4, and it has exactly one call site
(`Renderer::loop_enter`, updated in §3.1). The verbatim-Zig comment block above the fn
(`metal_impl.rs:461-475`) is retained as the R1 anchor.

### 3.3 The `draw_frame` split (generic.rs:5323) — §2.3

Mechanical: the current body's line 1 (`let self_ptr = self`) is deleted and the raw
pointer becomes the `draw_frame_raw(self_ptr, …)` parameter; the `&mut self`
`draw_frame` becomes a one-line wrapper. **Zero** lines of the ~200-line body change
(it is already written against `self_ptr`), so `check_comment_coverage.py` /
`check_test_counts.py` see only the new wrapper + doc.

### 3.4 `rustty-core/src/renderer_seam.rs` — re-route BOTH concurrent callers (§2.5)

**(a) Render thread — `Thread::drawFrame` `:1208`.** The render thread already holds
everything raw; only the draw call changes.

```rust
// } else {
//     self.renderer.drawFrame(false) catch |err| log.warn("error drawing err={}", .{err});
} else {
    // W5 §2.5 caller 1: RAW-self draw — the render thread must NOT establish a
    // protected whole-`Renderer` `&mut` (it would race the main-thread display
    // callback's draw_frame_raw under `draw_mutex`; the mutex cannot rescue a
    // protector established before the lock). `&raw mut (*(*self_).renderer).0` is a
    // raw place projection off the already-raw `renderer: *mut Renderer` — no reborrow.
    let rp: *mut rustty_renderer::Renderer = unsafe { &raw mut (*(*self_).renderer).0 };
    if let Err(_err) = unsafe { rustty_renderer::Renderer::draw_frame_raw(rp, false) } {
        // log.warn("error drawing err={}", .{err});
    }
}
```

**(b) Main-present forwarder — `Renderer::drawFrame` `:279-281`.** The Zig receiver is
`self: *Self` (a raw pointer); the port's `&mut self` was the divergence. Make it
raw-self so no protected `&mut Renderer(newtype)` (== `rustty_renderer::Renderer`
bytes) spans the call.

```rust
// pub fn drawFrame(self: *Self, sync: bool) !void   (generic.zig:1430)
//
// # Safety
// `self_` must be a valid, stable `*mut Renderer` (the Surface-embedded renderer,
// aliased raw by the render thread). RAW-self per Zig `*Self`; the `&mut self`
// receiver would be a protected `&mut` over the `rustty_renderer::Renderer` bytes,
// UB-concurrent with the render thread (W5_DESIGN §2.5 caller 2).
pub unsafe fn drawFrame(self_: *mut Renderer, sync: bool) -> Result<(), RendererError> {
    // self.0.draw_frame(sync)  →  raw-self draw (no protector spans the call)
    unsafe { rustty_renderer::Renderer::draw_frame_raw(&raw mut (*self_).0, sync) }
}
```

### 3.5 `Surface::draw` (core + embedded) + the C-ABI shim — finish the raw-self chain

**Core `Surface::draw` (`surface.rs:2791`).** Zig `draw(self: *Surface)`; raw-self so
the `&mut Surface` protector never covers the render thread's `renderer.0` writes.

```rust
// pub fn draw(self: *Surface) !void {   (Surface.zig)
//
// # Safety: `self_` is the stable Surface allocation (init's stability contract);
// RAW-self per Zig `*Surface` — an `&mut self` receiver would protect `self.renderer`
// (embedded by value) against the concurrent render thread (W5_DESIGN §2.5 caller 2).
pub unsafe fn draw(self_: *mut Surface<A>) -> Result<(), crate::renderer_seam::RendererError> {
    // Renderers are required to support `drawFrame` being called from the main
    // thread, so that they can update contents during resize.
    // try self.renderer.drawFrame(true);
    unsafe { crate::renderer_seam::Renderer::drawFrame(&raw mut (*self_).renderer, true)? };
    Ok(())
}
```

**Embedded `Surface::draw` (`embedded.rs:1390`).** Zig `draw(self: *Surface)`; raw-self
(`core_surface` is embedded by value, `:889`). The `refresh`/other embedded methods on
this struct that reach `core_surface` by value already use the raw `(*surface)`
dispatch idiom (`:386`), so this is consistent.

```rust
// pub fn draw(self: *Surface) void {
pub unsafe fn draw(self_: *mut Surface) {
    // self.core_surface.draw() catch |err| { log.err(...); return; };
    if let Err(_err) = unsafe { CoreSurface::draw(&raw mut (*self_).core_surface) } {
        // log.err("error in draw err={}", .{err});
    }
}
```

**C-ABI shim (`apprt-c/src/lib.rs:1154`).** `surface` is already a raw `*mut Surface`;
drop the auto-ref.

```rust
// (*surface).draw();   →   Surface::draw(surface);
unsafe { Surface::draw(surface) };
```

This chain is the *whole* main-thread present path; `rustty_surface_draw` is the sole
caller of embedded `draw`, and embedded `draw` the sole reacher of core `Surface::draw`
(§0), so no other call site is affected. The render thread's `Thread::drawFrame`
callers (`:987, :1270, :1293, :1351`) call `Thread::drawFrame` (already raw-self) and
are untouched.

---

## 4. Field-layout / receiver confirmations

- `Renderer.api : Metal` — confirmed `generic.rs:1278` (`pub api: Metal`). Field name
  **`api`** matches the Zig `@fieldParentPtr("api", self)`.
- `offset_of!(Renderer, api)` is a `const` and works on `repr(Rust)` structs; `api` is
  `pub` (all `Renderer` fields are, `:1164-1167`), so it is nameable from `metal_impl`.
- `Metal.layer : Option<IOSurfaceLayer>` — `:111`; `set_display_callback(&self, …)` →
  a shared borrow, concurrency-safe.
- `DisplayCallback = Option<unsafe extern "C-unwind" fn(*mut c_void)>` — `display_callback`
  matches exactly; `align(8)` in Zig is satisfied by the Rust fn-item ABI.

---

## 5. HEADLESS KAT — prove the trampoline end-to-end (no window server / live thread)

Goal: build a `Renderer` with a real (headless) `IOSurfaceLayer`, run `loop_enter`,
**fire the layer's `display` method directly** (the message CA/the present path sends
live), and assert `draw_frame` actually ran. No `NSWindow`, no `NSView`, no
`CVDisplayLink`, no second thread — single-threaded, runs under default parallel
`cargo test`, Metal-skips on a device-less host.

Placement: `generic.rs` `mod tests`, beside `golden_renders_cursor_fg_bg`
(it needs `golden_renderer` + `IOSurfaceLayer` + `msg_send`).

```rust
#[test]
fn loop_enter_display_callback_draws_a_frame() {
    use objc2::msg_send;
    use objc2_core_foundation::{CGPoint, CGRect, CGSize};

    let mut r = match golden_renderer(10, 3) {
        Some(r) => r,
        None => return, // Metal-skip
    };

    // Give the terminal some content so the draw is non-trivial (mirrors the golden).
    let mut mutex = Mutex::new();
    let mut t = Terminal::init(Global, opts(10, 3)).unwrap();
    { let mut s = t.vt_stream(); s.next_slice(b"Ab"); s.deinit(); }
    let st = state::State {
        mutex: &mut mutex, terminal: &mut t,
        inspector: None, preedit: None, mouse: state::Mouse::default(),
    };
    r.update_frame(&st, true).unwrap();

    // Inject a headless IOSurfaceLayer (offscreen ctor leaves layer = None). All
    // fields are pub. `IOSurfaceLayer::init()` is window-server-free (proven by the
    // W0 iosurface_layer tests).
    let layer = IOSurfaceLayer::init().expect("IOSurfaceLayer::init");
    // Size it so surface_size == size.screen (scale 1 ⇒ size_changed == false ⇒ the
    // frame draws the existing cells rather than taking the resize path).
    layer.as_ca_layer().setContentsScale(1.0);
    layer.as_ca_layer().setBounds(CGRect::new(
        CGPoint::new(0.0, 0.0),
        CGSize::new(r.size.screen.width as f64, r.size.screen.height as f64),
    ));
    r.api.layer = Some(layer);

    // Install the display callback via the real seam (container_of + set_display_callback).
    let self_ptr: *mut Renderer = &mut r;
    let api_ptr: *mut Metal = unsafe { &raw mut (*self_ptr).api };
    unsafe { Metal::loop_enter(api_ptr) };

    // Arm an observable: draw_frame(true) drives the sync completion trampoline, which
    // sets health = Healthy. Pre-set Unhealthy so a no-op / early-return fails loudly.
    r.health.store(crate::Health::Unhealthy);

    // Fire the ObjC `display` method — exactly what Core Animation sends on the main
    // thread during a needsDisplayOnBoundsChange commit. This drives
    // display_callback → draw_frame_raw(self_ptr, true).
    let disp_layer = r.api.layer.as_ref().unwrap();
    let _: () = unsafe { msg_send![&*disp_layer.layer, display] };

    // The trampoline ran a full sync frame: completion set health = Healthy …
    assert_eq!(r.health.load(), crate::Health::Healthy,
        "display → display_callback → draw_frame_raw must run a full frame");
    // … and present(sync) set the layer contents to the drawn IOSurface.
    let contents: Option<objc2::rc::Retained<objc2::runtime::AnyObject>> =
        unsafe { r.api.layer.as_ref().unwrap().layer.contents() };
    assert!(contents.is_some(), "sync present must set the layer contents");

    r.deinit();
}
```

Why the assertions are non-vacuous:

- `health == Healthy` is set **only** by `frame_completed(Healthy)` off the sync
  command-buffer completion — reached only if `draw_frame_raw` passed the
  `surface_size != 0` / `needs_redraw` gates and actually encoded+submitted a frame.
  The pre-armed `Unhealthy` means "callback never fired" or "early-returned" both fail.
- `contents.is_some()` proves `present(sync)` → `layer.set_surface_sync` ran, i.e. the
  full draw→present leg executed through the trampoline (not just entry).

This exercises the **real** `Metal::loop_enter` (container_of + `set_display_callback`)
and the **real** `display_callback` → `draw_frame_raw`. `Renderer::loop_enter`'s
display-link half is separately covered (offscreen `display_link == None` ⇒ the added
lines type-check and early-return; the live half is the `#[ignore]` on-device
`CVDisplayLink` KAT).

> **The KAT is single-threaded and does NOT — cannot — observe the concurrency UB.**
> It fires `display` on the test thread with no render thread running, so it proves the
> trampoline's *functional* leg (container_of → `set_display_callback` → `display` →
> `display_callback` → `draw_frame_raw` → a full frame) but exercises **zero**
> cross-thread aliasing. The concurrency soundness — the whole point of §2.5's raw-self
> re-route — is covered **solely** by the corrected `mirihar-chunk3b` candidate (§6),
> which is where the two-thread protected-`&mut` hazard is actually modelled and
> checked under SB/TB. Neither artifact substitutes for the other.

---

## 6. mirihar candidate — `needs_mirihar = true`

The existing `drawframe_shipped` proves the **same-thread nested** raw-self +
completion-trampoline shape. W5 introduces two things it does **not** cover, both
non-trivial per doctrine (`shaper-hook-sb-clean-interleave`,
`rust-mut-args-strongly-protected`): (a) **cross-thread concurrent** raw-self
`draw_frame_raw` calls serialised by `draw_mutex`, and (b) the **container_of
provenance round-trip** (`&raw mut (*rp).api → byte_sub → *mut c_void → back → deref
all fields`). Add one candidate to `.port/phase-2/mirihar-chunk3b/src/main.rs`
(argv-selected, non-FFI model — no Metal/ObjC):

- **`displaycb_concurrent`** (the SHIPPED post-fix shape → must be **CLEAN** under SB
  **and** TB): a `FakeRenderer { draw_mutex: Mutex, api: FakeApi, field: u64 }`; a
  raw-self `draw_frame_raw(rp: *mut FakeRenderer)` that locks `draw_mutex`, then does
  the narrow field mutations (a **plain** `(*rp).field` write, modelling `cells_rebuilt`)
  + a nested `buffer_completed(rp)` re-entry (mirroring the real body's transient
  `&mut`), then unlocks. Spawn **two threads** sharing one `*mut FakeRenderer` (wrapped
  `Send` via a raw-ptr newtype): **thread A** models the render thread — it reaches
  `draw_frame_raw(&raw mut (*(*t).renderer).0, …)` through a raw `renderer: *mut Newtype`
  wrapper (mirroring `renderer_seam.rs:1208` post-fix, §3.4a); **thread B** models the
  main present + display callback — `draw_frame_raw` reached raw-self through the
  container_of `display_cb`. The container_of half: `loop_enter(rp)` computes `&raw mut
  (*rp).api`, `byte_sub(offset_of!(FakeRenderer, api))`, stores the result as `*mut
  c_void` in a `Cell`, then `display_cb` reads it back and calls `draw_frame_raw`.
  **Both concurrent callers are raw-self — this is exactly the code §3.4/§3.5 ship.**
- **`displaycb_naive`** (the REJECTED / pre-fix shape → expected **UB**; the UB control
  that makes the split load-bearing, not cosmetic): models the code as it *actually
  stood before this fix* — **thread A on the `&mut self` receiver**
  (`(*rp).draw_frame(&mut self)`, the render thread at `renderer_seam.rs:1208` pre-fix)
  concurrent with **thread B on `draw_frame_raw`** (the raw display callback). Exactly
  **one** protected `&mut` receiver racing a raw draw + a plain-field write. Expected:
  UB under SB **and** TB (foreign write to a location under a live protected `&mut`).
  This is the shape the prior draft would have shipped, so it must FAIL — if instead it
  were CLEAN, the split would be cosmetic and the certification false-green.

Pass criterion (recorded in `mirihar-chunk3b/README.md`, "W5 candidates"):
`displaycb_concurrent` CLEAN under both `-Zmiri-stacked-borrows` and
`-Zmiri-tree-borrows` (with `-Zmiri-num-cpus` / thread interleaving);
`displaycb_naive` UB under both — proving the `draw_frame_raw` split (and the
raw-self re-route of *both* concurrent callers, not just the display callback) is
load-bearing. Because `displaycb_naive` models the render-thread `&mut` receiver that
§3.4a removes, `needs_mirihar = true` now certifies the **actually-shipped** shape.

---

## 7. The "un-ignore a W4-4 finale test" question

**No test gets un-ignored.** There is no `#[ignore]`d W4-4 finale test in the tree
(verified: the only ignored renderer test is `cvdisplaylink.rs:319`, the on-device
live-`CVDisplayLink` KAT, which requires a real display and stays ignored). W5's
finale is the **new** headless KAT (§5), which runs by default (Metal-skip on a
device-less host). Optionally, a `#[ignore]` on-device sibling can drive the same
`Metal::init(view)` → `loop_enter` → live `display` against a real `NSView` for the
windowing-integration milestone, mirroring the cvdisplaylink on-device pattern — but
that is not required for W5 to be complete.

---

## 8. Gates & disposition

- **build / clippy -D / fmt**: the `draw_frame` split is mechanical; `Metal::loop_enter`
  signature change has one call site (§3.1); `display_callback` is `extern "C-unwind"`
  (matches `DisplayCallback`); the raw-self re-route (§3.4/§3.5) changes 6 receivers
  across 4 crates, each with a fully-enumerated, closed caller set (§0, §2.5). All
  edits keep `unsafe fn` + `# Safety` docs and are `#[allow(dead_code)]`-free (all
  `pub`, per the render-complete precedent).
- **check_acyclic.sh**: the raw-self chain adds **no new crate edges** — every call
  follows an existing dependency direction (`apprt-c → apprt → core → rustty-renderer`,
  and intra-crate `metal_impl → generic::Renderer`). No cycle.
- **check_comment_coverage.py / check_test_counts.py**: the seam carries its verbatim
  Zig body (`:461-475`); each re-routed method keeps its verbatim-Zig comment; the new
  KAT adds one test; the split adds only a doc'd wrapper.
- **R1/R4/R6**: R1 — every reference line ported, no lossy `String`, and the raw-self
  receivers are *closer* to the Zig `*Self`/`*Surface`/`*Renderer` signatures than the
  prior `&mut self` (fidelity improves); R4 — rustty-renderer names no apprt-api type
  (the ctx is `*mut c_void`); R6 — the raw round-trip + the **two-thread** raw-self
  aliasing (both concurrent callers) PROVEN on `mirihar-chunk3b` (§6, with the
  `displaycb_naive` UB control certifying the split is load-bearing), the FFI `display`
  path under ASan via the KAT. `needs_mirihar = true`.

**`ready`.** The three adversary aliasing blockers are resolved: (1) both concurrent
production callers (render thread `:1208`, main present `:280`/`surface.rs:2795`) are
re-routed to `draw_frame_raw` via the raw-self chain (§2.5, §3.4, §3.5) — the change-set
now includes the `renderer_seam.rs`/`surface.rs`/`embedded.rs`/`apprt-c` edits it
previously omitted; (2) the `&mut self` `draw_frame` wrapper is restricted to the
single-threaded offscreen tests and is unreachable from any concurrent path; (3) the
`mirihar-chunk3b` `displaycb_concurrent` models the **shipped** both-raw shape (CLEAN)
and `displaycb_naive` the **pre-fix** render-thread-`&mut` shape (UB). After this lands,
`Metal::loop_enter` is no longer `unimplemented!`, and the last live-windowing seam is
closed. (The broader main-thread `Surface` `&mut self`-vs-live-render-thread sweep,
§2.5, is a separate port-wide follow-on and not a W5 blocker.)
