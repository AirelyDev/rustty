# W3 DESIGN — real Renderer swap (E1) + faithful renderer-Thread event loop (E2a/E2b)

Chunk **W3** of `docs/superpowers/plans/2026-07-08-macos-windowing-sub1.md`. All
work is **in `crates/rustty-core`** (plus two tiny, justified visibility bumps in
`crates/rustty-renderer`). No new crate edges (R4): core already deps
rustty-renderer + rustty-xev + rustty-crash, so E adds nothing new —
`check_acyclic.sh` stays green.

Reference (AUTHORITATIVE, read in full): `ghostty/src/renderer/Thread.zig`
(threadMain + every callback), `ghostty/src/Surface.zig` `draw`.

> **Reference-callback-list correction (authoritative).** The plan's E2 prose
> lists `armLoop/startDrawTimer/stopDrawTimer`. **These do not exist in
> `renderer/Thread.zig`** (grep-verified: `renderer/` has no `armLoop`,
> `startDrawTimer`, `stopDrawTimer`). The real private surface is:
> `threadMain_`, `setQosClass`, `syncDrawTimer` (this **is** the start/stop-draw-
> timer logic), `drainMailbox`, `changeConfig`, `drawFrame` (private helper),
> `wakeupCallback`, `drawNowCallback`, `drawCallback`, `renderCallback`,
> `cursorTimerCallback`, `cursorCancelCallback`, `stopCallback`,
> `cursorBlinkInterval`. This design ports **that** set; the plan names are
> treated as paraphrase.

---

## 0. Files & current state (verified line numbers)

- `crates/rustty-core/src/renderer_seam.rs` — the `Renderer` PLACEHOLDER struct
  (`:97`) + its 6 `unimplemented!()` methods (`surfaceInit :107`, `init :123`,
  `finalizeSurfaceInit :136`, `threadEnter :147`, `drawFrame :160`, `deinit
  :166`); the `RendererError` opaque struct (`:68`) with Display/Error impls
  (`:70`,`:76`); `Options<A>` (`:180`) with `config/font_grid/size` +
  apprt-typed `surface_mailbox/rt_surface/thread`; the `Thread<A>` struct
  **already fully declared** (`:229–337`) + `Flags` (`:317`) + `Default`
  (`:342`); the three `unimplemented!()` methods `Thread::init` (`:366`),
  `Thread::deinit` (`:384`), `Thread::threadMain(&mut self)` (`:393`).
- `crates/rustty-core/src/surface.rs` — the canonical init wiring
  (`Renderer::surfaceInit :1679`, `DerivedConfig::init :1802`, `Renderer::init
  :1810` with `Options{…}`, `Thread::init :1857`, the pre-spawn Copy-handle
  captures `renderer_wakeup/renderer_mailbox :1885–1886`, the field-by-field
  `ptr::write` block `:1924–2023`, the renderer-thread spawn via
  `RendererThreadArg :2303–2314`); `Surface::initErrdefers` (`:2481`) — the
  failed-`init` unwind whose `:2490` io half is already non-`&mut` (`ptr::read`)
  but whose `:2492` renderer half is a post-spawn whole-`&mut renderer_thread`
  (E2 converts it, §2.9a); `Surface::deinit` stop/join (`:2559` stop,
  `:2565` join, `:2573` threadEnter, `:2596` renderer_thread.deinit, `:2598`
  renderer.deinit); `Surface::draw` (`:2762`).
- `crates/rustty-renderer/src/generic.rs` — the **real** monomorphic
  `Renderer` (`Renderer::init(alloc: Global, options: Options) :1810`,
  `deinit(&mut self) :2093`, `surface_init<S>(rt) :2172`,
  `finalize_surface_init<S>(&mut self, rt) :2185`, `thread_enter<S>(&self, rt)
  :2197`, `thread_exit(&self) :2206`, `loop_enter(&mut self, draw_now: Async)
  :2219`, `loop_exit(&self) :2244`, `set_macos_display_id(&self,id) :2268`,
  `has_vsync(&self) :2285`, `set_focus(&mut self,b) :2303`,
  `set_visible(&self,b) :2325`, `mark_dirty(&mut self) :2340`,
  `set_font_grid(&mut self,g) :2353`, `set_screen_size(&mut self,s) :2411`,
  `change_config(&mut self, DerivedConfig) :2444`, `update_frame(…) :4348`,
  `draw_frame(&mut self, sync) :5306`, `has_animations(&self) :5142`
  **PRIVATE**, pub fields `search_matches/…_selected_match/…_dirty :1206+`);
  the top-level `Options{config,font_grid,size}` (`:752`); the error enum
  `generic::Error` (`:929`, macos_impl module, glob-exported as
  `generic::Error`) — derives `Debug` only, **no** Display/Error.
- `crates/rustty-xev/src` — `Async` (Copy, `notify(&self) :async.rs:363`,
  `unsafe wait<UD,CB>(self, *mut Loop, *mut Completion, *mut UD, CB) :async.rs:216`),
  `Timer` (`unsafe run<UD,CB>(self,…,next_ms,…) :timer.rs:76`,
  `reset :timer.rs:171`, `cancel :timer.rs:222`), `Loop`
  (`run(&mut self, RunMode) :kqueue.rs:590`, `stop(&mut self) :kqueue.rs:377`,
  `RunMode::UntilDone`), `Completion` (`state_of(&self) -> CompletionState`
  pub; `CompletionState::{Active,Dead}`), `CallbackAction::{Rearm,Disarm}`.

The `Thread<A>` struct fields, `RendererThreadArg` Send-newtype, the
construct-then-spawn wiring, and the `Surface::init`/`deinit` skeleton are all
**already in place** — W3 fills bodies, it does **not** move or redeclare the
struct.

---

## 1. E1 — swap the placeholder `Renderer` for the real `rustty_renderer::Renderer`

### 1.1 Type shape: newtype forwarder (not a bare alias)

A bare `pub use rustty_renderer::Renderer` cannot work: core's callsites use the
camelCase seam names (`surfaceInit/finalizeSurfaceInit/threadEnter/drawFrame`)
and core's `Renderer::init` takes `Options<A>` (3 apprt-typed fields the real
`generic::Options` refuses — naming them in rustty-renderer would cycle). Use
the plan's **newtype forwarder**:

```rust
// renderer_seam.rs — REPLACES the placeholder `pub struct Renderer;` (:97) and
// its 6 unimplemented methods (:99–169).

/// The renderer implementation — the monomorphic `rustty_renderer::Renderer`,
/// wrapped so core keeps the camelCase seam names + the `Options<A>` ctor whose
/// apprt-typed fields the real `generic::Options` cannot hold (surface-mailbox
/// seam). PORT NOTE (CORE_BLUEPRINT §5): reference `pub const Renderer = switch
/// (build_config.renderer) { .metal => GenericRenderer(Metal), … }`
/// (renderer.zig:38) — here the macOS/Metal monomorph.
pub struct Renderer(pub rustty_renderer::Renderer);
```

`.0` is `pub` so the same-crate `Thread<A>` callbacks reach the **full** real
method surface (`set_visible`, `update_frame`, `loop_enter`, …) directly through
`(*self.renderer).0`. Core's **out-of-module** callers only touch the 6
forwarders below.

`Thread<A>.renderer : *mut Renderer` (`:284`) and `Surface.renderer : Renderer`
(`surface.rs:325`) are unchanged — both name the newtype.

### 1.2 `RendererError` — alias the real error (recommended) + 2-line impl bump

The seam's opaque `RendererError` (`:68`) is used by:
`Surface::draw -> Result<(), RendererError>` (`surface.rs:2762`);
`InitError::Renderer(crate::renderer_seam::RendererError)` (`surface.rs:1376`)
with `From` (`:1407`) and `Display` delegation `err.fmt(f)` (`:1465`).

`generic::Error` derives **only `Debug`**. Two options:

- **(A, recommended) Alias + impl bump.** `pub type RendererError =
  rustty_renderer::generic::Error;` and add to `rustty-renderer/src/generic.rs`
  (inside `macos_impl`, right after the `enum Error` at `:929`):
  ```rust
  impl core::fmt::Display for Error {
      fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
          write!(f, "{self:?}")
      }
  }
  impl std::error::Error for Error {}
  ```
  Faithful (the reference error is an inferred union that already flows out of
  `drawFrame`), propagates the **real** variant (`MetalFailed`/`OutOfMemory`/
  `Defunct`/…), and leaves `surface.rs`'s From/Display **unchanged**. Delete the
  seam's `RendererError` struct + its Display/Error impls (`:61–76`).

- **(B, fallback, no rustty-renderer edit) map_err collapse.** Keep the opaque
  `RendererError` struct; forwarders do `.map_err(|_| RendererError)`. Loses the
  variant (matches today's opaque marker) but is less faithful. Use only if the
  impl bump is contested.

**Recommendation: (A).** It is the smaller net diff and the more faithful
error-propagation.

### 1.3 The 6 forwarders (replace `:99–169`)

```rust
impl Renderer {
    // surfaceInit — generic.zig:864; callsite surface.rs:1679.
    pub fn surfaceInit<S>(rt_surface: *mut S) -> Result<(), RendererError> {
        rustty_renderer::Renderer::surface_init(rt_surface)   // (A): Error == RendererError
    }

    // init — generic.zig:658; callsite surface.rs:1810. Map Options<A> -> the
    // real Options (config/font_grid/size); the apprt-typed surface_mailbox/
    // rt_surface/thread stay in core (surface-mailbox seam: the real renderer
    // does not store them — frame_completed's surface_mailbox push is deferred).
    // The moved-in `config` (real DerivedConfig) is consumed by the real init;
    // on its error the real init already runs the reference errdefer
    // (options.config.deinit()) internally — see generic.rs init body.
    pub fn init<A: apprt::App + 'static>(
        alloc: Allocator,
        options: Options<A>,
    ) -> Result<Renderer, RendererError> {
        let Options { config, font_grid, size, surface_mailbox, rt_surface, thread } = options;
        let _ = (surface_mailbox, rt_surface, thread); // held-in-core seam bits
        let inner = rustty_renderer::Renderer::init(
            alloc,
            rustty_renderer::generic::Options { config, font_grid, size },
        )?;
        Ok(Renderer(inner))
    }

    // finalizeSurfaceInit — generic.zig:873; callsite surface.rs (finalize).
    pub fn finalizeSurfaceInit<S>(&mut self, rt_surface: *mut S) -> Result<(), RendererError> {
        self.0.finalize_surface_init(rt_surface)
    }

    // threadEnter — generic.zig:881; callsite surface.rs:2573 (deinit re-enter).
    pub fn threadEnter<S>(&self, rt_surface: *mut S) -> Result<(), RendererError> {
        self.0.thread_enter(rt_surface)
    }

    // drawFrame — generic.zig:1430; callsite surface.rs:2766 (Surface::draw)
    // + the Thread `drawFrame` helper (§2.7).
    pub fn drawFrame(&mut self, sync: bool) -> Result<(), RendererError> {
        self.0.draw_frame(sync)
    }

    // deinit — generic.zig:800; callsites surface.rs:2496/2598 + init errdefers.
    pub fn deinit(&mut self) {
        self.0.deinit();
    }
}
```

> **`options.config.deinit()` errdefer note (PORT NOTE at `:117`).** The
> placeholder flagged that when `init` lands it must `config.deinit()` on the
> error path. That errdefer lives **inside the real `Renderer::init`** (it owns
> the moved `DerivedConfig`). The forwarder therefore does **not** re-run it —
> doing so would be a double-free. Update the `:117` PORT NOTE to record that the
> errdefer is now discharged by the real init, not the forwarder.

### 1.4 `Surface::draw` wiring (`surface.rs:2762`)

**Already wired** — `self.renderer.drawFrame(true)?` → the forwarder →
`self.0.draw_frame(true)`, the real synchronous main-thread present. No edit,
except confirm the return type stays `Result<(), RendererError>` (unchanged
under (A); under (A) `RendererError` is now `generic::Error`, and the existing
`?`/`From` still compile because `Surface::Error`/`InitError` are not on this
path — `draw` returns `RendererError` directly).

### 1.5 `Surface::init` / `Surface::deinit` renderer wiring

**All already wired** and correct against the forwarders:
- `Renderer::surfaceInit(rt_surface)` (`:1679`) — matches forwarder.
- `Renderer::init(alloc, Options{config: renderer_config, font_grid, size,
  surface_mailbox, rt_surface, thread})` (`:1810`) — matches. The `renderer_impl
  .deinit()` errdefers (`:1871/1903`) call the forwarder `deinit`.
- `self.renderer.threadEnter(self.rt_surface)` (`:2573`) — forwarder.
- `self.renderer.deinit()` (`:2496/2598`) — forwarder.

No `Surface::init`/`deinit` edits for E1. (E2 changes only the spawn callsite —
§2.9.)

---

## 2. E2a/E2b — fill `Thread<A>` (init, deinit, threadMain + all callbacks)

Everything below is **inside the existing `impl<A: apprt::App + 'static>
Thread<A>` block** in `renderer_seam.rs` (`:353`). The struct/fields at
`:229–337` are untouched. Ports `renderer/Thread.zig` line-for-line.

### 2.1 Module constants + `must_draw_from_app_thread` binding

```rust
// Thread.zig:19-20
const DRAW_INTERVAL: u64 = 8; // 120 FPS
const CURSOR_BLINK_INTERVAL: u64 = 600;

// Thread.zig:22-30. `@hasDecl(apprt.App, "must_draw_from_app_thread")` — the
// embedded/macOS App (this milestone) does NOT declare it (only GTK does, which
// is deferred), so the probe resolves to `false`. When GTK lands it flips this
// to `apprt::App::MUST_DRAW_FROM_APP_THREAD` (a new default-false trait const).
// Both `drawFrame` arms (§2.7) stay live regardless.
const MUST_DRAW_FROM_APP_THREAD: bool = false;
```

### 2.2 THE ALIASING DISCIPLINE (the crux) — raw-self everywhere

After the spawn (`surface.rs:2303`) the child thread holds `*mut Thread<A>` for
its whole life; the xev loop hands **that same raw pointer** back into every
callback as `userdata: *mut Thread<A>` (xev `wait/run/reset` take `userdata: *mut
UD`, `UD = Thread<A>`, `async.rs:216`/`timer.rs:76`). So the faithful port is
**already** raw-self — the reference's `self_: ?*Thread` callback params map to
`*mut Thread<A>`.

Rules (the `[ADV-FIX F1/F2]` + search/io precedent, and
`rust-mut-args-strongly-protected`):

1. **`threadMain` takes `self_: *mut Thread<A>`**, NOT `&mut self`. No
   `let t = &mut *self_;` whole-struct reborrow anywhere. (E2b signature change,
   §2.9.)
2. Every callback is a **free `unsafe fn cb(self_: *mut Thread<A>, …) ->
   CallbackAction`** (or a module fn) taking raw `self_`; inside, only **narrow
   per-field** projections `(*self_).<field>` and `&(*self_).<field>` /
   `&mut (*self_).<field>` — never `&mut *self_`/`&*self_` whole.
3. xev registration args stay raw: `self.wakeup.wait(&raw mut (*self_).loop_,
   &raw mut (*self_).wakeup_c, self_, wakeupCallback)` — `loop`/`c` raw (the loop
   re-enters `c` across ticks; a `&mut` re-read would be UB), `userdata = self_`.
4. **Main-thread (post-spawn) may touch ONLY**: the Copy `wakeup`/`draw_now`
   `Async` handles captured pre-spawn (`surface.rs:1885`), the `*const mailbox`,
   and — at deinit — a **narrow shared** `&(*self).renderer_thread.stop` for
   `stop.notify()`. NEVER `&mut renderer_thread`. `Async::notify(&self)` reads
   only the mach-port handle (Copy), so the narrow shared ref into the pinned
   `renderer_thread.stop` is compatible with the child's raw-self narrow use
   (the `callback_shipped` wakeup-notify shape, already proven).
5. `Completion::state()` checks use the pub `state_of(&self)` on a **narrow**
   `&(*self_).draw_c` etc. (disjoint field, transient shared ref).

The naive shape (whole `&mut *self_` in threadMain/callbacks) is UB; proven on
the mirihar harness (§4).

### 2.3 `init` (fill `:366`)

Ports `Thread.zig:125–181` verbatim. All xev handles init with `?` (the
reference `try` → `xev::Error`, the declared return `Result<Thread<A>,
xev::Error>`). The mailbox is `thread::Mailbox::create(alloc)` (already REAL);
`app_mailbox` is stored; `config = thread::DerivedConfig::init(config)`. The
reference `alloc` field collapses (Box ownership — the `:227` PORT NOTE);
`flags` takes `Flags::default()` (`:342`). Completion fields take
`xev::Completion::default()` (the `= .{}` reference defaults; confirm
`Completion: Default` — it is, per the `= .{}` field defaults the struct doc
records).

```rust
pub fn init(
    alloc: Allocator,
    config: &rustty_config::Config,
    surface: *mut A::Surface,
    renderer_impl: *mut Renderer,
    state: *mut State,
    app_mailbox: crate::app::Mailbox<A>,
) -> Result<Thread<A>, xev::Error> {
    let mut loop_ = xev::Loop::init()?;              // errdefer loop_.deinit()  (see note)
    let mut wakeup_h = xev::Async::init()?;
    let mut stop_h = xev::Async::init()?;
    let mut render_h = xev::Timer::init()?;
    let mut draw_h = xev::Timer::init()?;
    let mut draw_now = xev::Async::init()?;
    let mut cursor_timer = xev::Timer::init()?;
    let mailbox = thread::Mailbox::create(alloc)?;   // real BlockingQueue::create
    Ok(Thread {
        loop_, wakeup: wakeup_h, wakeup_c: xev::Completion::default(),
        stop: stop_h, stop_c: xev::Completion::default(),
        render_h, render_c: xev::Completion::default(),
        draw_h, draw_c: xev::Completion::default(), draw_active: false,
        draw_now, draw_now_c: xev::Completion::default(),
        cursor_h: cursor_timer, cursor_c: xev::Completion::default(),
        cursor_c_cancel: xev::Completion::default(),
        surface, renderer: renderer_impl, state, mailbox, app_mailbox,
        config: thread::DerivedConfig::init(config),
        flags: Flags::default(),
    })
}
```

> **errdefer parity (CONFIRMED: explicit chain required).** rustty-xev has
> **no `impl Drop`** for `Loop`/`Timer`/`Async` (grep-verified) — every teardown
> is an explicit `deinit()` syscall. So the reference `errdefer h.deinit()`
> ordering (`Thread.zig:135–163`) does **not** happen for free on the `?` early
> return; it MUST be reproduced explicitly: bind each `?` in a `match` and, on
> the `Err` arm, `deinit()` the earlier-initialized locals in LIFO order (loop_,
> wakeup_h, stop_h, render_h, draw_h, draw_now, cursor_timer), then return the
> error — the `surface.rs:1866` errdefer-chain precedent. The tidy chained-`?`
> body shown above is the success skeleton; the fill wraps it in the LIFO match
> chain (no aliasing concern — pre-spawn, single-threaded).

### 2.4 `deinit` (fill `:384`)

Ports `Thread.zig:185–196` verbatim. `&mut self` (non-consuming — the caller
`surface.rs:2596` still owns the field). Called only post-join (no alias):

```rust
pub fn deinit(&mut self) {
    self.stop.deinit();
    self.wakeup.deinit();
    self.render_h.deinit();
    self.draw_h.deinit();
    self.draw_now.deinit();
    self.cursor_h.deinit();
    self.loop_.deinit();
    // "Nothing can possibly access the mailbox anymore, destroy it."
    self.mailbox.destroy(/* alloc collapsed into the raw Box */);
}
```
`mailbox.destroy` — the reference `self.mailbox.destroy(self.alloc)`; the
`thread::Mailbox` (BlockingQueue) `create/destroy` pair (already REAL). Since
`deinit` is `&mut self` and runs post-join, the field accesses here are plain
`&mut self.<field>` (no aliasing hazard — the child is dead).

### 2.5 `threadMain` (E2b — retype `:393`) + `threadMain_`

```rust
// Thread.zig:199-205. E2b: RAW self (was &mut self). NO whole reborrow.
pub unsafe fn threadMain(self_: *mut Thread<A>) {
    // self.threadMain_() catch |err| log.warn("error in renderer err={}", err);
    if let Err(_err) = unsafe { Self::threadMain_(self_) } {
        // log.warn("error in renderer err={}", _err);
    }
}
```

`threadMain_` ports `Thread.zig:207–263` with raw-self throughout:

```rust
unsafe fn threadMain_(self_: *mut Thread<A>) -> Result<(), xev::Error> {
    // defer log.debug("renderer thread exited") — a scope guard / trailing log.

    // Thread.zig:213-215 name the thread (Darwin). macOS crate ⇒ unconditional.
    unsafe { rustty_os::macos::pthread_setname_np(c"renderer".as_ptr()) };

    // Thread.zig:217-222 crash metadata. `.type = .renderer`, `.surface = <core
    // Surface>`. The real Renderer dropped surface_mailbox (seam), so obtain the
    // ThreadStateSurface target from the Thread's apprt surface via `.core()`:
    let core_surface = unsafe { (*(*self_).surface).core() }; // *mut Surface<A>
    crash::sentry::setThreadState(crash::sentry::ThreadState {
        type_: crash::sentry::Type::Renderer,
        surface: core_surface, // *const dyn ThreadStateSurface (core Surface impls it)
    });
    let _thread_state = ThreadStateGuard; // defer crash.sentry.thread_state = null

    // Thread.zig:225 QoS.
    unsafe { Self::setQosClass(self_) };

    // Thread.zig:227-230 loopEnter/loopExit. `@hasDecl(Renderer,"loopEnter")` is
    // TRUE for Metal ⇒ the real `loop_enter(draw_now)`. R4: the reference passes
    // `self` and reads `&thr.draw_now`; the real signature takes the Copy
    // `draw_now: Async` directly. loopExit on defer.
    unsafe { (*(*self_).renderer).0.loop_enter((*self_).draw_now)? };
    let _loop_exit = LoopExitGuard(unsafe { (*self_).renderer }); // defer loop_exit()

    // Thread.zig:232-236 threadEnter/threadExit.
    unsafe { (*(*self_).renderer).0.thread_enter((*self_).surface)? };
    let _thread_exit = ThreadExitGuard(unsafe { (*self_).renderer }); // defer thread_exit()

    // Thread.zig:238-241 arm the async handlers. Raw loop/c, userdata = self_.
    unsafe {
        (*self_).wakeup.wait(
            &raw mut (*self_).loop_, &raw mut (*self_).wakeup_c, self_, wakeupCallback::<A>);
        (*self_).stop.wait(
            &raw mut (*self_).loop_, &raw mut (*self_).stop_c, self_, stopCallback::<A>);
        (*self_).draw_now.wait(
            &raw mut (*self_).loop_, &raw mut (*self_).draw_now_c, self_, drawNowCallback::<A>);
    }

    // Thread.zig:243-244 initial wakeup.
    unsafe { (*self_).wakeup.notify()? };

    // Thread.zig:246-254 start cursor blink.
    unsafe {
        (*self_).cursor_h.run(
            &raw mut (*self_).loop_, &raw mut (*self_).cursor_c,
            cursorBlinkInterval(), self_, cursorTimerCallback::<A>);
    }

    // Thread.zig:256-257 start the draw timer.
    unsafe { Self::syncDrawTimer(self_) };

    // Thread.zig:259-262 run. `_ = try self.loop.run(.until_done);`
    unsafe { (*(&raw mut (*self_).loop_)).run(xev::RunMode::UntilDone)? };
    Ok(())
}
```

> **Guard structs.** The reference `defer`s (loopExit, threadExit,
> clearThreadState, the two debug logs) become small RAII guards
> (`ThreadStateGuard` already exists at `surface.rs:1514`; add `LoopExitGuard`/
> `ThreadExitGuard` holding `*mut Renderer`, each `Drop` calling the `&self`
> `loop_exit`/`thread_exit` on `(*r).0`). Because these run at scope end (after
> `loop.run` returns, i.e. post-stop), the child no longer races the main thread
> — narrow raw derefs in the guards are safe.

### 2.6 `setQosClass`, `syncDrawTimer`, `changeConfig`, `cursorBlinkInterval`

- `setQosClass(self_: *mut Thread<A>)` — `Thread.zig:265–293`. macOS crate ⇒ no
  `isDarwin` guard. Reads `(*self_).flags.visible`/`.focused` (narrow) → picks
  `rustty_os::macos::QosClass::{Utility,UserInitiated,UserInteractive}`, calls
  `rustty_os::macos::setQosClass(class)` (Result → log on err). Reference is
  `*const Thread`; port as raw `self_` (narrow field reads).
- `syncDrawTimer(self_: *mut Thread<A>)` — `Thread.zig:295–334`. The
  `skip:`/`break :skip` control flow → a labeled block or an early-return helper.
  `@hasDecl(Renderer,"hasAnimations")` is TRUE for Metal ⇒ call
  `(*(*self_).renderer).0.has_animations()` (**requires the visibility bump,
  §3**). Match `(*self_).config.custom_shader_animation` (the thread
  `DerivedConfig`): `Always`→animate, `True`→animate iff `flags.focused`,
  `False`→skip. On skip: `(*self_).draw_active = false; return`. Else:
  `(*self_).draw_active = true`; if `Completion::state_of(&(*self_).draw_c) ==
  Active` return; else `(*self_).draw_h.run(&raw mut (*self_).loop_, &raw mut
  (*self_).draw_c, DRAW_INTERVAL, self_, drawCallback::<A>)`.
- `changeConfig(self_, config: *const thread::DerivedConfig)` — `Thread.zig:494`
  → `(*self_).config = *config;` (Copy).
- `cursorBlinkInterval() -> u64` — `Thread.zig:712–725`. `runningOnValgrind`:
  rustty has no valgrind shim, so the fn `return`s `CURSOR_BLINK_INTERVAL`. The
  `* 5` valgrind branch is **NOT silently dropped** — it is preserved verbatim as
  a commented-out reference block + a PORT NOTE recording why it collapses (no
  `std.valgrind` equivalent), so a future valgrind probe can re-arm it:
  ```rust
  fn cursorBlinkInterval() -> u64 {
      // if (std.valgrind.runningOnValgrind() > 0) {
      //     // If we're running under Valgrind, the cursor blink adds enough
      //     // churn that it makes some stalls annoying unless you're on a
      //     // super powerful computer, so we delay it.
      //     //
      //     // This is a hack, we should change some of our cursor timer
      //     // logic to be more efficient:
      //     // https://github.com/ghostty-org/ghostty/issues/8003
      //     return CURSOR_BLINK_INTERVAL * 5;
      // }
      // PORT NOTE: rustty has no `std.valgrind.runningOnValgrind` shim, so the
      // valgrind `* 5` branch above is inert (never taken) and kept as a
      // verbatim comment; the fn returns the plain constant. Re-arm the branch
      // if a `rustty_os` valgrind probe is added.
      CURSOR_BLINK_INTERVAL
  }
  ```

### 2.7 `drawFrame` private helper (`Thread.zig:498–517`)

```rust
fn drawFrame(self_: *mut Thread<A>, now: bool) {
    unsafe {
        if !(*self_).flags.visible { return; }                 // narrow
        if !now && (*(*self_).renderer).0.has_vsync() { return; } // narrow &self.api
        if MUST_DRAW_FROM_APP_THREAD {
            // self.app_mailbox.push(.{ .redraw_surface = self.surface }, .{ .instant });
            let _ = (*self_).app_mailbox.push(
                crate::app::Message::RedrawSurface((*self_).surface),
                crate::app::Timeout::Instant, // match the app::Mailbox timeout enum
            );
        } else {
            if let Err(_err) = (*(*self_).renderer).0.draw_frame(false) {
                // log.warn("error drawing err={}", _err);
            }
        }
    }
}
```
Both arms compile (`app::Mailbox::push` + `Message::RedrawSurface` exist,
`app.rs:952/896`). `MUST_DRAW_FROM_APP_THREAD == false` this milestone, so the
else arm runs; the push arm is dead-but-live (faithful, no `unimplemented!`).

### 2.8 The callbacks (`Thread.zig:519–709`)

All are **module-level `unsafe fn`s generic over `A`** (not methods — to keep
raw `self_` and satisfy the ZST-callback bound: `Async::wait`/`Timer::run`
require the `CB` be a zero-sized fn-item, so pass the bare fn item
`wakeupCallback::<A>`). Signature shape matches xev:
`fn cb<A>(self_: *mut Thread<A>, _l: *mut Loop, _c: *mut Completion, r:
Result<(), WaitError|RunError|CancelError>) -> CallbackAction`.

- `wakeupCallback` (`:519`): `r` err → log + `Rearm`. Else `drainMailbox(self_)`
  (log on err), then `renderCallback::<A>(self_, null, null, Ok(()))`, then
  `Rearm`. (The commented-out coalesce timer stays a verbatim comment.)
- `drawNowCallback` (`:560`): `r` err → log + `Rearm`. Else `drawFrame(self_,
  true)`, `Rearm`.
- `drawCallback` (`:578`): `r` `catch unreachable`. `drawFrame(self_, false)`;
  if `(*self_).draw_active` re-arm `draw_h.run(…DRAW_INTERVAL…drawCallback)`;
  return `Disarm`.
- `renderCallback` (`:602`): `r` `catch unreachable`. If `!(*self_).flags.visible`
  return `Disarm`. Else call `update_frame` — **note the real signature
  (`generic.rs:4348`) is `update_frame(&mut self, state: &state::State,
  cursor_blink_visible: bool) -> Result<(), VtOutOfMemory>`, NOT `*mut State` /
  `RendererError`**: the `state` field is `*mut State` but `update_frame` takes a
  **shared** `&State` (it mutates only through the stored `*mut Terminal`), so
  deref it narrow-shared: `(*(*self_).renderer).0.update_frame(&*(*self_).state,
  (*self_).flags.cursor_blink_visible)`. The propagated error is the
  `VtOutOfMemory` type (`rustty_vt::pagelist::OutOfMemory`, aliased), so the
  log-on-err arm formats that (`// log.warn("error updating frame err={}", err)`
  over the VtOutOfMemory value, not a RendererError). Then `drawFrame(self_,
  false)`, `Disarm`.
- `cursorTimerCallback` (`:632`): `r` err → `Canceled`⇒`Disarm`, else log +
  `unreachable`. Toggle `(*self_).flags.cursor_blink_visible`;
  `(*self_).wakeup.notify()` (`catch {}`); re-arm `cursor_h.run(…
  cursorBlinkInterval()… cursorTimerCallback)`; `Disarm`.
- `cursorCancelCallback` (`:668`): userdata is `void`/`null` (the reference
  `?*void`) — port as `fn(_ud: *mut (), …)`; map the cancel error set, `Disarm`.
- `stopCallback` (`:700`): `r` `catch unreachable`; `(*self_).loop_.stop()` via
  narrow `&mut (*self_).loop_`; `Disarm`.

**`drainMailbox`** (`Thread.zig:337–492`) — raw `self_`, ports every match arm
verbatim over the REAL renderer + REAL `thread::Mailbox`:

- Darwin `@autoreleasepool` wrap. **CONFIRMED: rustty-core does NOT dep objc2**
  (only rustty-renderer/os do). Rather than add an objc2 edge to core just for a
  Darwin memory-hygiene pool (not semantics), port this as a **deferred leaf**: a
  no-op scope carrying the **full verbatim reference comment** (the exact 8 lines
  `// There's probably a more elegant way to do this... / … / const pool = if
  (builtin.os.tag.isDarwin()) @import("objc").AutoreleasePool.init() else void; /
  defer if (builtin.os.tag.isDarwin()) pool.deinit();`, `Thread.zig:338–346`),
  and this deferral is **logged in `.port/phase-2/DEFERRALS.md`** (a W3
  renderer-Thread entry) at fill time — both are mandatory, not optional: the
  comment-coverage gate anchors on the verbatim block and the deferral audit
  reads DEFERRALS.md. (Alternative if pressed: add the external objc2 dep — no
  crate-graph cycle, but heavier than the leaf.)
- `while let Some(message) = (*(*self_).mailbox).pop()` — narrow `&*mailbox`
  (external, internally-synced BlockingQueue).
- Arms map onto the real `message::Message` variants (all exist, `message.rs:49`):
  `Crash`⇒panic; `Visible(v)`/`Focus(v)`/`ResetCursorBlink`/`FontGrid{…}`/
  `Resize(v)`/`ChangeConfig{thread,r#impl}`/`SearchViewportMatches(v)`/
  `SearchSelectedMatch(v)`/`Inspector(v)`/`MacosDisplayId(v)`. Each calls the
  real renderer method through `(*(*self_).renderer).0.<m>()`:
  `set_visible`/`set_focus`/`update_frame`/`set_font_grid`/`set_screen_size`/
  `change_config`/`set_macos_display_id`.
  **The `.visible` arm's visible-regain `updateFrame` (`Thread.zig:366`) is the
  SAME `update_frame(&*(*self_).state, …) -> VtOutOfMemory` shared-deref +
  VtOutOfMemory-log shape as `renderCallback` above — not `*mut State` /
  `RendererError`.** Plus the pub fields
  `search_matches`/`search_selected_match`/`search_matches_dirty`. The
  `.font_grid` arm's `grid.set.deref(grid.old_key)` uses `SharedGridSet::deref`.
  The `.change_config` arm's `alloc.destroy(config.thread/impl)` collapses to
  `Box` drop (the `Message::ChangeConfig{thread: Box<…>, r#impl: Box<…>}` owns
  them, `message.rs:100`); after the two `change_config`s, `syncDrawTimer(self_)`.
  The `.search_*` arms' `m.arena.deinit()` collapse to `Option`/`Vec` drop (the
  arena-collapse precedent, `message.rs`). The `@hasDecl(Renderer,
  "setMacOSDisplayID")` probe is TRUE for Metal ⇒ the `.macos_display_id` arm
  calls `set_macos_display_id`.

### 2.9 The spawn callsite (E2b — `surface.rs:2311`)

`threadMain` is now `unsafe fn threadMain(self_: *mut Thread<A>)`. Change the
closure body `:2311`:

```rust
// was: unsafe { (*arg.0).threadMain() }
unsafe { crate::renderer_seam::Thread::<A>::threadMain(arg.0) }
```
Exactly the io-thread precedent (`:2340` `termio::Thread::threadMain(args.thread,
args.io)`). `RendererThreadArg` (`:1489`), the Send wrapper, and the Builder are
unchanged.

`Surface::deinit`'s `self.renderer_thread.stop.notify()` (`:2559`) is the
narrow-shared main-thread stop (§2.2 rule 4) — already correct.

### 2.9a `initErrdefers` — the post-spawn renderer-thread teardown (SECOND surface.rs edit)

**`initErrdefers` (`surface.rs:2481`, the failed-`init` unwind) has a POST-SPAWN
`&mut renderer_thread` that E2's aliasing discipline forbids.** Line `:2492`:

```rust
// errdefer render_thread.deinit(); (Surface.zig:575)
(*self_).renderer_thread.deinit();          // <-- whole &mut renderer_thread
```

This path is **reachable with the renderer child LIVE**: the renderer thread is
spawned (`:2303–2314`) and the io thread spawned (`:2342`) BEFORE the
`perform_action(.set_title)` calls (`:2366/2407/2431`); the reference confirms
`_ = try rt_app.performAction(.set_title)` runs after both spawns
(`Surface.zig:743`). Any of those `try`s failing fires `initErrdefers` while the
renderer child is still running its `threadMain` loop over `*mut Thread<A>`.
Taking `&mut (*self_).renderer_thread` there:

1. **contradicts §2.2 rule 4** (`NEVER &mut renderer_thread`) and the old §2.9
   claim of a single 2311 edit; and
2. is **inconsistent with the sibling io half** (`:2490`), which deliberately
   uses `std::ptr::read(&raw const (*self_).io_thread).deinit()` to avoid the
   `&mut` (the failed-`init` read-back-out contract, §initErrdefers doc).

**FIX (this IS a required E2 `surface.rs` edit).** Convert `:2492` to the same
non-`&mut` `ptr::read` form as the io half, and — because the child is LIVE —
stop+join it first (the reference's bare `errdefer render_thread.deinit()`
without a stop/join is a **latent UAF the port would otherwise reproduce on both
threads**; io at `:2490` has the identical latent bug):

```rust
// errdefer render_thread.deinit(); (Surface.zig:575)
// The renderer child is LIVE on this post-spawn unwind path (spawn :2303, then
// perform_action(.set_title) can fail :2366/2407/2431 — Surface.zig:743). Stop
// and join it BEFORE deinit so deinit()'s teardown does not race/UAF the child,
// then read the field out by-value (no &mut — §2.2 rule 4), matching the io
// half at :2490. (The reference errdefer omits the stop/join — a latent UAF we
// fix on the port; see the ADV-FIX note.)
{
    let rt = std::ptr::read(&raw const (*self_).renderer_thread);
    rt.stop.notify();   // narrow Copy handle (Async::notify(&self))
    // join the spawned handle if init still holds it on this path; else the
    // detached child observes stop and exits its loop.run(.until_done).
    rt.deinit();
}
```

> **Latent-UAF scope note.** Strictly, both the io (`:2490`) and renderer
> (`:2492`) errdefers reproduce the reference's join-less teardown. E2 owns the
> renderer half (this edit). The io half is termio-owned and out of W3 scope, but
> flag it in the fold summary so the sibling can apply the same stop+join.

**So E2 requires TWO `surface.rs` edits: the `:2311` spawn callsite (§2.9) and
this `:2492` `initErrdefers` conversion (§2.9a).**

---

## 3. Required `rustty-renderer` edits (2, both tiny + justified)

1. **`generic::Renderer::has_animations` → `pub`** (`generic.rs:5142`). The
   reference method is effectively public (reached via `@hasDecl` from the
   thread); `syncDrawTimer` (core) now calls it cross-crate. Change `fn
   has_animations` → `pub fn has_animations`. (Its only other caller `:5415` is
   unaffected.)
2. **`impl Display + std::error::Error for generic::Error`** (§1.2 option A) —
   2 impls after `:929`.

Both are within rustty-renderer (no new edge). If either is contested, the
fallbacks are §1.2(B) for the error, and a `pub(crate)` + core-visible
re-export is *not* possible cross-crate, so `has_animations` must be `pub`
(or re-exposed via an existing pub method — none fits; `pub` is correct).

---

## 4. mirihar candidates — `renderthread_naive` (UB) / `renderthread_shipped` (CLEAN)

Add to `.port/phase-2/mirihar-chunk3b/src/main.rs`. Models the **construct-then-
spawn + xev-loop-callback-reentry** shape that §2's threadMain/callbacks realize,
which the existing `search_*`/`callback_*` candidates do **not** cover (they
model the detached-spawn producer + child→main re-entry, but not a child
`threadMain` that holds `self` across a loop firing callbacks that re-deref
`self`, concurrent with the main thread's narrow stop/wakeup/mailbox touches).

xev is **stubbed** (non-FFI): the "loop" is a channel-driven tick sequence that
invokes the stored callback with the raw userdata pointer — reproducing xev's
"userdata is a raw `*mut Thread` re-handed to each callback" contract.

```rust
// ============================================================================
// W3 §2.2: renderer-Thread construct-then-spawn + xev-loop callback re-entry.
//
//   * A `RThread` is stored in a PINNED Box (the reference `self.renderer_thread`
//     field of the pinned Surface), lowered to raw `tp: *mut RThread` at birth.
//   * The child runs `thread_main(tp)` holding `tp` for its whole life; the
//     stubbed loop fires callbacks with the SAME raw `tp` (xev userdata).
//     Callbacks re-deref `tp` and call `(*renderer).draw_frame()` + drain the
//     external mailbox + narrow field writes.
//   * CONCURRENTLY the MAIN thread produces (wakeup.notify + mailbox.push) and,
//     at deinit, does stop.notify — touching ONLY Copy handles + the *const
//     mailbox + a narrow `&(*tp).stop`. Sequenced by mpsc for a stable verdict.
//
// naive: thread_main binds `let t = &mut *tp` (whole) held across the loop; a
//        callback reborrows `&mut *tp` (sibling) -> pops t; t is re-used after
//        -> use-after-pop. (Also: the main producer reborrows `&mut *tp` whole.)
// shipped: thread_main + callbacks use raw `tp` with NARROW field access only;
//          main uses narrow `&(*tp).wakeup/.stop` + `*const mailbox`.
//
// Pass: shipped CLEAN under BOTH -Zmiri-stacked-borrows and -Zmiri-tree-borrows;
//       naive reports UB under at least one.
// ============================================================================

struct RRenderer { draws: u32 }
impl RRenderer {
    fn draw_frame(&mut self) { self.draws = self.draws.wrapping_add(1); }
    fn has_vsync(&self) -> bool { false }
}

// xev.Async analogue (Copy handle; notify(&self) reads only an Arc). Inline
// fields of RThread == renderer_thread.wakeup/.stop/.draw_now.
#[derive(Clone, Copy)]
struct RAsync { shared: *const AtomicU32 }
impl RAsync { fn notify(&self) { unsafe { (*self.shared).fetch_add(1, AOrd::SeqCst); } } }

struct RThread {
    // xev handles (Copy) — inline; main touches wakeup/stop/draw_now only.
    wakeup: RAsync,
    stop: RAsync,
    draw_now: RAsync,
    // external mailbox (BlockingQueue) reached via *const (outside RThread's
    // SB/TB coverage) — the decoupled-producer lineage (search_shipped finding).
    mailbox: *const Ext,
    renderer: *mut RRenderer,     // *rendererpkg.Renderer
    draw_active: bool,
    cursor_blink_visible: bool,
    visible: bool,
    field: u32,                   // stand-in for misc narrow field writes
}

// ---- callbacks: raw self_, NARROW field access (SHIPPED shape) ----
unsafe fn render_cb_shipped(self_: *mut RThread) {
    if !(*self_).visible { return; }
    let r = (*self_).renderer;               // narrow field read
    (*r).draw_frame();                       // (*renderer).draw_frame()
    // drawFrame helper: narrow reads
    if !(*self_).visible { return; }
    let _ = (*(*self_).renderer).has_vsync();
    (*self_).field = (*self_).field.wrapping_add(1); // narrow write
}
unsafe fn wakeup_cb_shipped(self_: *mut RThread) {
    // drainMailbox: pop the EXTERNAL mailbox via *const (narrow copy of the ptr)
    let mb: *const Ext = (*self_).mailbox;
    (*mb).push(0);                           // model a drain touch
    render_cb_shipped(self_);                // renderCallback(t, undefined, …)
}
unsafe fn cursor_cb_shipped(self_: *mut RThread) {
    (*self_).cursor_blink_visible = !(*self_).cursor_blink_visible; // narrow
    (*self_).wakeup.notify();                                       // Copy handle
}

// ---- callbacks: whole &mut *self_ reborrow (NAIVE shape) ----
unsafe fn wakeup_cb_naive(self_: *mut RThread) {
    let t: &mut RThread = &mut *self_;       // SIBLING whole reborrow -> pops outer
    let mb = t.mailbox; (*mb).push(0);
    if t.visible { (*t.renderer).draw_frame(); t.field = t.field.wrapping_add(1); }
}

struct SendRT(*mut RThread);
unsafe impl Send for SendRT {}

fn run_renderthread(shipped: bool) {
    let wake = Arc::new(AtomicU32::new(0));
    let stopf = Arc::new(AtomicU32::new(0));
    let dnow = Arc::new(AtomicU32::new(0));
    let ext = Box::new(Ext { inner: SMutex::new(Vec::new()) });
    let ext_ptr: *const Ext = &*ext;
    let mut rndr = Box::new(RRenderer { draws: 0 });
    let rndr_ptr: *mut RRenderer = &raw mut *rndr;

    let mut boxed: Box<RThread> = Box::new(RThread {
        wakeup: RAsync { shared: Arc::as_ptr(&wake) },
        stop:   RAsync { shared: Arc::as_ptr(&stopf) },
        draw_now: RAsync { shared: Arc::as_ptr(&dnow) },
        mailbox: ext_ptr,
        renderer: rndr_ptr,
        draw_active: false,
        cursor_blink_visible: false,
        visible: true,
        field: 0,
    });
    // ONE transient &mut at birth, lowered to raw (§2.4 / search_shipped).
    let tp: *mut RThread = &raw mut *boxed;

    let (go_tx, go_rx): (Sender<u8>, Receiver<u8>) = channel(); // main -> child ticks
    let (ack_tx, ack_rx): (Sender<u8>, Receiver<u8>) = channel();

    let send = SendRT(tp);
    let handle = std::thread::Builder::new().name("renderer".into()).spawn(move || {
        let s = send; let go = go_rx; let ack = ack_tx;
        unsafe {
            if shipped {
                // thread_main(self_: *mut RThread): NO whole reborrow. Loop the
                // stubbed xev: each tick fires a callback with the raw userdata.
                loop {
                    match go.recv() { Ok(0) => wakeup_cb_shipped(s.0),
                                      Ok(1) => cursor_cb_shipped(s.0),
                                      _ => break }
                    // (No DCE-defeat write here: the callbacks already mutate
                    // observable state — renderer.draws, mailbox, (*self_).field —
                    // so DCE cannot elide them. A raw `(s.0 as *mut u8)` byte write
                    // would clobber RThread's first field (`wakeup: RAsync`), a
                    // mis-narrowed access that corrupts the very Copy handle the
                    // shipped shape is proving CLEAN — deliberately omitted.)
                    ack.send(1).unwrap();
                }
            } else {
                // NAIVE: whole &mut held across the loop; callback reborrows &mut
                // *s.0 (sibling) -> pops t; t re-used after -> use-after-pop.
                let t: &mut RThread = &mut *s.0;
                loop {
                    match go.recv() { Ok(0) => { wakeup_cb_naive(s.0);          // pops t
                                                 t.field = t.field.wrapping_add(1); } // UAP
                                      Ok(1) => { t.cursor_blink_visible = !t.cursor_blink_visible; }
                                      _ => break }
                    ack.send(1).unwrap();
                }
            }
        }
    }).unwrap();

    // let the child arm (establish its tag) — first tick.
    go_tx.send(0).unwrap(); let _ = ack_rx.recv();

    // MAIN producer (steady state) — between child ticks.
    if shipped {
        let wk: RAsync = unsafe { (*tp).wakeup };  // Copy handle read (narrow field copy)
        wk.notify();
        let mb: *const Ext = unsafe { (*tp).mailbox }; // Copy *const read
        unsafe { (*mb).push(42) };
    } else {
        let m: &mut RThread = unsafe { &mut *tp };  // whole reborrow -> pops child tag
        m.field = m.field.wrapping_add(1);
        unsafe { (*m.mailbox).push(42) };
    }
    go_tx.send(1).unwrap(); let _ = ack_rx.recv(); // cursor tick

    // DEINIT: stop.notify via NARROW shared &(*tp).stop, then join (child dead).
    if shipped {
        let st: &RAsync = unsafe { &(*tp).stop };   // narrow shared into pinned struct
        st.notify();
    } else {
        let m: &mut RThread = unsafe { &mut *tp };   // whole reborrow (naive)
        m.stop.notify();
    }
    go_tx.send(2).unwrap();                          // stop tick -> loop break
    handle.join().unwrap();

    let draws = unsafe { (*(*tp).renderer).draws };
    println!("renderthread_{}: draws={draws} field={}",
             if shipped { "shipped" } else { "naive" }, unsafe { (*tp).field });
    drop(boxed); drop(rndr); drop(ext);
    let _ = (wake, stopf, dnow);
}
```

Wire the dispatch in `main()` (next to `search_*`):
```rust
"renderthread_shipped" => { run_renderthread(true);  return; }
"renderthread_naive"   => { run_renderthread(false); return; }
```

**Run / expected:**
```
cd .port/phase-2/mirihar-chunk3b
for c in renderthread_shipped renderthread_naive; do
  cargo +nightly miri run -- $c                                  # SB (default)
  MIRIFLAGS=-Zmiri-tree-borrows cargo +nightly miri run -- $c    # TB
done
```
- `renderthread_shipped` → **CLEAN** under both (raw-self narrow child + narrow-
  shared/Copy main).
- `renderthread_naive` → **UB** (use-after-pop: the child's callback `&mut *s.0`
  and/or the main `&mut *tp` pops the held `&mut RThread`; the subsequent
  `t.field`/`m.field` is the flagged access).

> The shipped candidate is the structural proof the §2 fill must match. If
> shipped is NOT clean, the fill's raw-self shape is wrong — do NOT ship the
> core code until shipped is green under both models (MIRI_EXCEPTIONS.md rule).

---

## 5. Surface::draw bounded-pump KAT

`Surface::draw` → `renderer.drawFrame(true)` → the real `draw_frame(true)` (the
"drawFrame from the main thread" contract, `Surface.zig:884`). A full core
`Surface::draw` unit test needs the whole apprt mock + a live Metal surface
(`Surface::init` runs `Metal::init_offscreen` then the windowing host attach),
which is the windowing-integration milestone. So the **bounded-pump KAT** lands
at the renderer's existing **offscreen golden harness** (generic.rs
`macos_impl::tests`, the `draw_frame(true)` harness at `:6279–6403`), extended
to a bounded N-frame pump:

```rust
// EXTENDS generic.rs macos_impl::tests offscreen harness (:6279+). Proves the
// Surface::draw path (drawFrame(true), main-thread present) is safe to pump.
#[test]
fn draw_frame_bounded_pump() {
    let (mut r, _guards) = build_offscreen_renderer(); // the existing :6316 setup
    for _ in 0..8 {                                    // bounded — never a live loop
        r.draw_frame(true).expect("main-thread draw_frame(true)");
        assert_eq!(r.health.load(), crate::Health::Healthy);
    }
    // deinit path clean.
    r.deinit();
}
```

**Coverage boundary (explicit — this KAT does NOT drive `threadMain`).** The
bounded pump calls `draw_frame(true)` **directly** on the offscreen renderer. It
does **not** spin the real `threadMain` event loop, and it observes no
callback-driven draw (no `wakeupCallback → drainMailbox → renderCallback`, no
draw/cursor timer firing). It proves only the `Surface::draw → drawFrame(true)`
main-thread-present leg is safe to pump repeatedly. **The callback-driven
`threadMain`/wakeup/timer path is proven ELSEWHERE**, by two artifacts, NOT by
this KAT:
- the xev-stubbed **mirihar `renderthread_shipped`** candidate (§4), which drives
  a `thread_main(tp)` loop firing callbacks that re-deref `tp` and observes
  `renderer.draws++` — the aliasing + callback-reentry proof; and
- **`cargo build --workspace`**, which type-checks the real `threadMain` +
  every callback against xev/renderer signatures.
There is **no** unit test exercising the live `Surface::draw → threadMain` loop,
because that needs a live-Metal + apprt windowing host (the windowing-integration
milestone), unavailable in unit tests. This is a deliberate, stated gap — Q6
("drive threadMain + observe a draw") is satisfied by the mirihar shipped
candidate + build, not by §5.

Rationale for the KAT that IS here: `Surface::draw` adds no logic over
`drawFrame(true)` (`surface.rs:2766` is a one-line forward), so the
renderer-level bounded pump is the faithful, runnable proof of that leg; the core
forward is covered by `cargo build` + the existing golden. (If a core-side smoke
is wanted, a `#[cfg(test)]` assertion that `Surface::<TestApp>::draw` type-checks
against the forwarder — a compile check — is the ceiling without the windowing
host.)

The count delta this test adds is registered in `check_test_counts.py`'s expected
map (§6).

---

## 6. Gates & verification

- `cargo build --workspace` — the newtype/forwarder + Thread fill + spawn
  callsite compile; the two rustty-renderer visibility edits.
- `cargo test --workspace` — existing renderer offscreen golden + the new
  bounded-pump KAT; core surface tests (TestApp) still green.
- `cargo clippy --workspace --all-targets -- -D warnings` — raw-ptr derefs are
  in `unsafe` blocks; the dead `MUST_DRAW_FROM_APP_THREAD` push arm needs no
  allow (both arms are reachable code paths under `if const`).
- `cargo fmt --all -- --check`.
- `.port/tools/gates/check_acyclic.sh` — **green** (no new edge; E is core-local
  over already-present deps).
- `.port/tools/gates/check_test_counts.py` — bump the expected count for the new
  `draw_frame_bounded_pump` test.
- `.port/tools/gates/check_comment_coverage.py` — every ported reference line
  (Thread.zig callbacks + Surface.zig draw) carries its `// Thread.zig:NNN`
  anchor comment; the per-arm drainMailbox comments; the newtype/forwarder PORT
  NOTEs.
- **mirihar**: `renderthread_shipped` CLEAN under SB **and** TB;
  `renderthread_naive` UB — recorded before the core fill ships.

---

## 7. Precise edit list

**`crates/rustty-core/src/renderer_seam.rs`**
1. `:61–76` delete `RendererError` struct + Display/Error impls; add `pub type
   RendererError = rustty_renderer::generic::Error;` (§1.2 A).
2. `:97–169` replace placeholder `Renderer` struct + 6 unimplemented methods
   with the newtype `pub struct Renderer(pub rustty_renderer::Renderer);` + the
   6 forwarders (§1.1, §1.3). Update the `:117` errdefer PORT NOTE (§1.3).
3. Add module consts `DRAW_INTERVAL`/`CURSOR_BLINK_INTERVAL`/
   `MUST_DRAW_FROM_APP_THREAD` (§2.1).
4. `:366` fill `Thread::init` (§2.3). `:384` fill `Thread::deinit` (§2.4).
5. `:393` retype + fill `Thread::threadMain` → `unsafe fn threadMain(self_: *mut
   Thread<A>)` + add `threadMain_` (§2.5).
6. Add `setQosClass`/`syncDrawTimer`/`changeConfig`/`cursorBlinkInterval`
   (§2.6), the `drawFrame` helper (§2.7), `drainMailbox` (§2.8), the 7 callbacks
   + guard structs (`LoopExitGuard`/`ThreadExitGuard`) (§2.5, §2.8).
7. Imports: `rustty_crash as crash` (or the existing crash path used by
   surface.rs), `rustty_os`, `objc2` (autoreleasepool, if wired), the message
   enum, `xev::{RunMode, CallbackAction, CompletionState}`.

**`crates/rustty-core/src/surface.rs`** (TWO edits, both E2)
8. `:2311` change the spawn closure body to `Thread::<A>::threadMain(arg.0)`
   (§2.9).
8a. `:2492` `initErrdefers` — convert the post-spawn `(*self_).renderer_thread
    .deinit()` whole-`&mut` to the non-`&mut` `ptr::read(&raw const (*self_)
    .renderer_thread)` form + stop.notify()/join first (child is LIVE on this
    unwind path), matching the io half at `:2490` (§2.9a). Required by §2.2
    rule 4.

**`crates/rustty-renderer/src/generic.rs`**
9. `:5142` `fn has_animations` → `pub fn has_animations`.
10. after `:929` add `Display` + `std::error::Error` for `Error` (§1.2 A).
11. extend the offscreen `macos_impl::tests` harness with
    `draw_frame_bounded_pump` (§5).

**`.port/phase-2/mirihar-chunk3b/src/main.rs`**
12. add `RRenderer`/`RAsync`/`RThread` + `run_renderthread` + the shipped/naive
    callbacks + the two `main()` dispatch arms (§4).

---

## 8. Resolved facts (verified) + one remaining leaf

**Verified during design (no longer open):**
- **xev has no `Drop`** for `Loop`/`Timer`/`Async` → `Thread::init` uses the
  explicit LIFO errdefer chain (§2.3, confirmed).
- **`app::Mailbox::push` timeout** = `rustty_datastruct::blocking_queue::Timeout`
  (re-exported into `crate::app`); the reference `.instant` → **`Timeout::Instant`**
  (blocking_queue.rs:24) for the `MUST_DRAW_FROM_APP_THREAD` arm.
- **crash import**: `use rustty_crash as crash;` (surface.rs:141) — add the same
  `use` to renderer_seam.rs; `crash::sentry::{setThreadState, ThreadState, Type}`
  + `Type::Renderer` (sentry.rs:96) all exist; core's `Surface` impls
  `ThreadStateSurface`.
- **core lacks objc2** → `drainMailbox`'s `@autoreleasepool` is a deferred leaf
  (§2.8, confirmed).

**R1-leaf handling (both preserved verbatim, not dropped):**
- **`cursorBlinkInterval` valgrind branch** (`Thread.zig:713`) — rustty has no
  `std.valgrind` shim, so the `* 5` branch is inert; it is **kept verbatim as a
  commented-out reference block + PORT NOTE** (§2.6), never silently removed. The
  fn returns the plain `CURSOR_BLINK_INTERVAL`.
- **`drainMailbox` `@autoreleasepool`** (`Thread.zig:339–346`) — core lacks
  objc2; ported as a **deferred no-op leaf** carrying the verbatim reference
  comment (the `const pool = @autoreleasepool …; defer pool.deinit();` block,
  §2.8) and **logged in `.port/phase-2/DEFERRALS.md`** as a W3 renderer-Thread
  deferral at fill time. Comment-coverage gate sees the anchored verbatim lines.

**Disposition: ready.**
