# CORE Blueprint — rustty-core (Surface.zig 6036 / App.zig 604 / surface_mouse.zig 336)

Design review (2026-07-02) for the core milestone. Grounded in the actual reference files and the
landed crates (apprt-api 20t, termio 44t, crash 12t, renderer 19t, vt search thread, font F4).
Core is weakly test-gated (1 reference test) ⇒ THIS blueprint + per-chunk scoped-G3b + chunked
review ARE the quality gate.

**Adversarially reviewed: ACCEPT-WITH-FIXES (2026-07-02) — the 7 fixes are folded in below**
(marked `[ADV-FIX n]`). The genericity/seam/deferral architecture survived every check; grep
counts, cycle claims, and the AppMailbox/Timeout/none.zig calls were independently confirmed.

Headline discovery: unlike termio stage-2, core needs almost NO verbatim-comment deferrals.
The dependency-placeholder pattern (termio `xev_seam`: inhabited structs, `unimplemented!()`
methods) lets ~95% of Surface.zig port as LIVE, compiling, reviewable Rust; runtime panics are
confined to the placeholder leaves (GPU Renderer, xev handles, uninhabited Inspector). The only
verbatim-comment deferrals in core are `activateInspector`/`deactivateInspector` (~45 loc, they
construct the uninhabited `Inspector`).

---

## 1. Genericity strategy (make-or-break)

Zig compiles ONE apprt backend at comptime; apprt-api committed to GENERIC (not dyn) `App`/
`Surface` traits. The parameter `A: rustty_apprt_api::App` threads through core exactly once and
stops at the mailbox seam.

Top-level signatures (exact):

```rust
// rustty-core/src/app.rs  (App.zig)
pub struct App<A: rustty_apprt_api::App + 'static> {
    pub alloc: Allocator,                            // allocator_api2 Global (termio precedent)
    pub surfaces: Vec<*mut A::Surface>,              // SurfaceList = ArrayListUnmanaged(*apprt.Surface)
    pub focused: bool,
    pub focused_surface: Option<*mut Surface<A>>,
    pub mailbox: Queue<A>,                           // Mailbox.Queue (by value, field `mailbox: Mailbox.Queue`)
    pub font_grid_set: rustty_font::SharedGridSet,
    pub last_notification_time: Option<std::time::Instant>,
    pub last_notification_digest: u64,
    pub config_conditional_state: rustty_config::conditional::State,
    pub first: bool,
}

/// App.Message (App.zig:527) — names *Surface and *apprt.Surface, hence generic.
pub enum Message<A: rustty_apprt_api::App + 'static> {
    OpenConfig,
    NewWindow(NewWindow<A>),                         // struct NewWindow<A> { parent: Option<*mut Surface<A>> }
    Close(*mut Surface<A>),
    Quit,
    SurfaceMessage(rustty_apprt_api::surface::SurfaceMessageEnvelope<'static, *mut Surface<A>>),
    RedrawSurface(*mut A::Surface),
}
// Cross-thread queue of raw pointers — reference threading contract, documented invariant:
unsafe impl<A: rustty_apprt_api::App> Send for Message<A> {}

pub type Queue<A> = rustty_datastruct::BlockingQueue<Message<A>, 64>;   // App.Mailbox.Queue

/// App.Mailbox (App.zig:560) — the app-thread mailbox handle other threads hold.
pub struct Mailbox<A: rustty_apprt_api::App + 'static> {
    pub rt_app: *mut A,
    pub mailbox: *mut Queue<A>,
}
impl<A: ...> Mailbox<A> {
    pub fn push(&self, msg: Message<A>, timeout: Timeout) -> Size {
        let result = unsafe { &*self.mailbox }.push(msg, timeout);
        unsafe { &*self.rt_app }.wakeup();           // "Wake up our app loop"
        result
    }
}

// rustty-core/src/surface.rs  (Surface.zig)
pub struct Surface<A: rustty_apprt_api::App + 'static> {
    pub id: u64,
    pub alloc: Allocator,
    pub app: *mut App<A>,
    pub rt_app: *mut A,                              // rt_app: *apprt.runtime.App
    pub rt_surface: *mut A::Surface,                 // rt_surface: *apprt.runtime.Surface
    pub font_grid_key: rustty_font::shared_grid_set::Key,
    pub font_size: rustty_font::face::DesiredSize,
    pub font_metrics: rustty_font::Metrics,
    pub font_size_adjusted: bool,
    pub renderer: crate::renderer_seam::Renderer,            // Phase-3 GPU placeholder (§5)
    pub renderer_state: rustty_renderer::state::State,
    pub renderer_thread: crate::renderer_seam::Thread<A>,    // Phase-3 placeholder shape (§5)
    pub renderer_thr: Option<std::thread::JoinHandle<()>>,   // std.Thread (undefined-until-spawn → Option; deviation documented)
    pub mouse: Mouse,
    pub keyboard: Keyboard,
    pub pressed_key: Option<rustty_vt::input::KeyEvent>,     // utf8: Vec<u8> — owned, copy with utf8 = vec![]
    pub last_binding_trigger: u64,
    pub io: rustty_termio::Termio<SurfaceMailbox<A>>,        // ← the concrete M (below)
    pub io_thread: rustty_termio::thread::Thread,
    pub io_thr: Option<std::thread::JoinHandle<()>>,
    pub inspector: Option<*mut rustty_inspector::Inspector>, // uninhabited placeholder (§4)
    pub size: rustty_renderer::size::Size,
    pub config: DerivedConfig,                               // core-local, Surface.zig:290
    pub config_conditional_state: rustty_config::conditional::State,
    pub child_exited: bool,
    pub focused: bool,
    pub selection_scroll_active: bool,
    pub readonly: bool,
    pub command_timer: Option<std::time::Instant>,
    pub search: Option<Search>,                              // vt search::Thread + JoinHandle — fully real
    pub last_bell_time: Option<std::time::Instant>,
}

/// apprt.surface.Mailbox — the reference struct could not live in apprt-api (names core);
/// it lands HERE and implements the AppMailbox trait. Re-exported as `Surface::Mailbox`
/// (Surface.zig:15 `pub const Mailbox = apprt.surface.Mailbox`).
pub struct SurfaceMailbox<A: rustty_apprt_api::App + 'static> {
    pub surface: *mut Surface<A>,
    pub app: crate::app::Mailbox<A>,
}
impl<A: ...> rustty_apprt_api::surface::AppMailbox for SurfaceMailbox<A> {
    type Surface = *mut Surface<A>;
    fn push(&self, msg: SurfaceMessage<'static>, timeout: Timeout) -> Size {
        // "Surface message sending is actually implemented on the app thread..." (rewrap)
        self.app.push(Message::SurfaceMessage(SurfaceMessageEnvelope {
            surface: self.surface, message: msg }), timeout)
    }
}

impl<A: ...> rustty_apprt_api::CoreSurfaceHandle for Surface<A> {   // already-landed seam
    fn rt_surface_ptr(&self) -> *mut c_void { self.rt_surface.cast() }
}
impl<A: ...> rustty_crash::sentry::ThreadStateSurface for Surface<A> {  // un-defer (§6 C11)
    fn screen_size(&self) -> (u32, u32) { (self.size.screen.width, self.size.screen.height) }
    fn grid_size(&self)   -> (u32, u32) { let g = self.size.grid(); (g.columns, g.rows) }
    fn cell_size(&self)   -> (u32, u32) { (self.size.cell.width, self.size.cell.height) }
}
```

**Where `A` propagates:** `App<A>`, `Message<A>`, `Mailbox<A>`, `Surface<A>`,
`SurfaceMailbox<A>`, `renderer_seam::Thread<A>` (holds `surface: *mut A::Surface` +
`app_mailbox: app::Mailbox<A>`, mirroring renderer/Thread.zig fields), and the already-generic
termio instantiations `Termio<SurfaceMailbox<A>>` / `StreamHandler<SurfaceMailbox<A>>` /
`Options<'_, SurfaceMailbox<A>>` / `ThreadData<SurfaceMailbox<A>>`.

**Where it STOPS (explosion audit):** rustty-vt is NEVER parameterized by `A` — vt's `Stream`
is generic over its Handler and `StreamHandler<M>` already erases core behind `M`. rustty-font,
rustty-config, rustty-renderer (state/message/cell), rustty-input, rustty-crash
(`ThreadState` erases to `*const dyn ThreadStateSurface`), rustty-inspector: all non-generic.
Monomorphization cost = one instantiation per apprt backend per binary = exactly Zig's comptime
model. `A: 'static` is required by `crash::setThreadState`'s dyn erasure (S: 'static) — benign
(backend types own no borrows).

**apprt.Surface → core Surface (the `core()` edge; [ADV-FIX 4] actual sites
App.zig:140/193/231/285/477/502/511):** two options —
- **(1: RECOMMENDED)** additive `type Core;` on the apprt-api `Surface` trait + typed accessors
  (§2). Core constrains at impl blocks: `impl<A> App<A> where A: apprt_api::App, A::Surface:
  apprt_api::Surface<App = A, Core = Surface<A>>`. Self-referential bound — prototype in C0.
- **(2: fallback)** `fn core_ptr(&self) -> *mut c_void` mirroring the landed
  `CoreSurfaceHandle::rt_surface_ptr` opaque-pointer discipline; core casts. Use only if (1)
  fights the trait solver.

---

## 2. apprt-api growth points — the exact additive list

Grep results: Surface.zig calls `rt_app.performAction` ×77 (all Action variants already ported in
`action.rs`; NO trait change needed for them) + `rt_app.keyboardLayout` ×1; `rt_surface.`:
getCursorPos ×12, setClipboard ×5, getContentScale ×5, supportsClipboard ×2, getTitle ×1,
getSize ×1, defaultTermioEnv ×1, close ×1, clipboardRequest ×1. App.zig: `rt_app.performAction`
×14, `rt_app.wakeup` ×1 (exists), `rt_app.config` field ×1, `rt_surface.core()` ×1,
`rt_surface.rtApp()` ×2, `rt_app.performIpc` ×0 (exists for cli).

**`App` trait additions** (seam.rs; snake_case per the file's landed convention, reference name
documented at each):

```rust
/// `rt_app.config.keybind.set.getEvent(event)` — App.zig:340. The backend owns its Config
/// (embedded.zig:125 `config: Config`).
fn config(&self) -> &rustty_config::Config;
/// `rt_app.keyboardLayout()` — Surface.zig:3271; embedded.zig:219 (macOS-only detection).
fn keyboard_layout(&self) -> rustty_input::keyboard::Layout;
```

**`Surface` trait additions** (signatures from apprt/embedded.zig, the first concrete backend;
errors use the landed opaque `ApprtError` growth-point marker, same rationale as
`perform_action`):

```rust
type Core;                                                          // rustty-core Surface<Self::App> (§1)
fn core(&self) -> *mut Self::Core;                                  // embedded.zig:633
fn rt_app(&self) -> *mut Self::App;                                 // embedded.zig:637 rtApp
fn close(&self, process_alive: bool);                               // :641 (needsConfirmQuit result)
fn get_content_scale(&self) -> Result<ContentScale, ApprtError>;    // :650
fn get_size(&self) -> Result<SurfaceSize, ApprtError>;              // :654
fn get_title(&self) -> Option<&std::ffi::CStr>;                     // :658 `?[:0]const u8` (borrow tied to &self)
                                                                    // [ADV-FIX 2.4] embedded takes *Surface (non-const);
                                                                    // &self is fine (body is a field read) — document at the method.
fn supports_clipboard(&self, clipboard_type: Clipboard) -> bool;    // :662
/// [ADV-FIX 1] returns !bool — whether the request STARTED (embedded.zig:672-697 `return false/true`);
/// load-bearing at Surface.zig:5822 startClipboardRequest (performable-keybind pass-through).
fn clipboard_request(&self, clipboard_type: Clipboard,
                     state: ClipboardRequest) -> Result<bool, ApprtError>;    // :672
fn set_clipboard(&self, clipboard_type: Clipboard,
                 contents: &[ClipboardContent<'_>],
                 confirm: bool) -> Result<(), ApprtError>;          // :735 (order per callsites 2201/2317)
fn get_cursor_pos(&self) -> Result<CursorPos, ApprtError>;          // :760
fn default_termio_env(&self)
    -> Result<std::collections::HashMap<String, String>, ApprtError>;  // :953 std.process.EnvMap
                                                                        // == rustty_termio::command::EnvMap == rustty-os getEnvMap
```

All payload types (`ContentScale`, `SurfaceSize`, `CursorPos`, `Clipboard`, `ClipboardContent`,
`ClipboardRequest`, `IMEPos`, `ColorScheme`) already exist in `structs.rs`. Additive, R1-safe.

**performAction call shape from core** (the 91 callsites; `Surface<A>` is the `C:
CoreSurfaceHandle`):

```rust
let _ = unsafe { &*self.rt_app }.perform_action(
    Target::Surface(&*self),                       // Target<'_, Surface<A>> (action.rs:42)
    Action::CellSize(CellSize { width: size.cell.width, height: size.cell.height }),
)?;    // `_ = try rt_app.performAction(.{ .surface = self }, .cell_size, .{...})` — Surface.zig:685
```

Full set of Action variants exercised by core (already ported, for the reviewer's checklist).
[ADV-FIX 4] Regenerated mechanically against the reference (the original list missed 7 exercised
variants and used binding-action names for one): cell_size, size_limit, initial_size, set_title,
**PromptTitle** (binding names prompt_surface_title/prompt_tab_title map onto it), pwd,
mouse_shape, mouse_visibility, mouse_over_link, open_url, render, quit, new_window, new_tab,
close_tab, goto_tab, move_tab, new_split, goto_split, goto_window, resize_split,
equalize_splits, toggle_split_zoom, reset_window_size, toggle_maximize, toggle_fullscreen,
float_window, toggle_window_decorations, toggle_tab_overview, toggle_command_palette,
toggle_quick_terminal, toggle_visibility, toggle_background_opacity, secure_input, inspector,
desktop_notification, color_change, config_change, reload_config, open_config,
close_all_windows, close_window, quit_timer, key_sequence, key_table, ring_bell,
progress_report, command_finished, search_total, search_selected, start_search, present_terminal,
show_gtk_inspector, check_for_updates, undo, redo, show_on_screen_keyboard, readonly,
**renderer_health** (Surface.zig:1681), **scrollbar** (:1692), **selection_changed** (:2351),
**show_child_exited** (:1243/:1270), **set_tab_title**, **end_search**,
**copy_title_to_clipboard** (:5065). All verified already ported in action.rs (230/254/259/274/
349/366/383/397) — no trait change needed; chunk reviews must check dispatch arms against THIS list.

---

## 3. The App Mailbox (resolves apprt-api's opaque Timeout/Size + termio un-defers)

App.zig:560: `Mailbox { rt_app: *apprt.App, mailbox: *Queue }`, `Queue = BlockingQueue(Message,
64)`, `push(msg, timeout: Queue.Timeout) Queue.Size` + `rt_app.wakeup()`. rustty-datastruct
already defines `blocking_queue::Timeout` (`Instant | Forever | Ns(u64)`) and `Size = u32` at
MODULE level (not per-instantiation) — so the faithful resolution of `App.Mailbox.Queue.Timeout`
IS the concrete datastruct type.

**Trait change (small, pre-core, R1-safe — the assoc types were explicitly opaque growth
placeholders):**

```rust
pub trait AppMailbox {
    type Surface;      // stays associated (core's *mut Surface<A>)
    fn push(&self, msg: Message<'static>,
            timeout: rustty_datastruct::blocking_queue::Timeout)
        -> rustty_datastruct::blocking_queue::Size;
}
```

- Drop `type Timeout`/`type Size` (concrete now). Generic termio code can then CONSTRUCT
  timeouts (`Timeout::Instant` / `Timeout::Forever`), which associated types made impossible
  without ugly factory methods.
- `Message<'_>` → `Message<'static>`: a storing push cannot accept an arbitrary lifetime. This
  is the established precedent — termio's `mailbox::Queue = BlockingQueue<Message<'static>, 64>`
  ("the cross-thread queue stores the 'static instantiation"). Verified: every existing termio
  push payload is owned ('static-compatible): `SetTitle([u8;256])`, `ClipboardWrite{req}` built
  from `WriteReq::init` (Alloc/Small), `PwdChange` (Alloc), `ChildExited`, `DesktopNotification`
  (owned arrays), etc. `ChangeConfig(&'a Config)` is only ever DIRECT-called
  (`App.updateConfig` → `surface.core().handleMessage(...)`), never queued, so `handleMessage`
  keeps `Message<'_>` while the queue stores `'static`.

**Un-defer verdicts:**
- `stream_handler::surfaceMessageWriter` — UN-DEFER with core: the "opaque core Timeout" blocker
  dissolves (concrete `Timeout::Instant`). Its slow path unlocks/relocks the caller-held
  renderer mutex, so it ALSO needs the mutex decision (next bullet). Both land in chunk C-T.
- `mailbox::send` caller-held-mutex pattern + `rendererMessageWriter` + `Termio::queueMessage`
  locked path — UN-DEFER with core by DECIDING the mutex model now: add a guard-less, Zig-shaped
  mutex `rustty_zigstd::thread::Mutex` (`lock()`/`try_lock()`/`unlock()`, no RAII guard; built on
  std `Mutex<bool>`+`Condvar`, zero new deps) and retype `renderer::State.mutex` from
  `*mut std::sync::Mutex<()>` to `*mut rustty_zigstd::thread::Mutex`. Rationale: Surface.zig is
  RIDDLED with manual patterns std's guard cannot express (`imePoint` locks/unlocks without
  defer; surfaceMessageWriter's unlock→push(forever)→relock inversion; `mouseRefreshLinks`
  "must be called with mutex held" across function boundaries). One retype unblocks core + the
  termio deferral family (`changeConfig`, `resize`, `sizeReport*`, `focusGained`,
  `resetSynchronizedOutput`, `clearScreen`, `scrollViewport`, `jumpToPrompt`,
  `colorSchemeReport*` — mutex+StreamHandler-blocked only, no xev). xev-blocked Termio paths
  (`init`/`threadEnter`/`processOutput*`) STAY deferred (D6).

---

## 4. Inspector seam

What Surface.zig actually uses (grep-complete):
1. `inspectorpkg.KeyEvent` (= `widgets/key.zig Event`) — constructed literally in keyCallback
   (:2667), fields `.event/.binding/.pty` mutated in maybeHandleBinding (:3059) and encodeKey
   (:3238), `deinit(alloc)`. This type is threaded through `keyCallback`/`maybeHandleBinding`/
   `encodeKey` SIGNATURES — it cannot be deferred without gutting the input hot path.
2. `inspectorpkg.Inspector` — `init(alloc)`/`deinit(alloc)`/`recordKeyEvent(alloc, ev)`;
   `?*Inspector` stored on Surface and mirrored into `renderer_state.inspector`
   (activate/deactivateInspector, :890-934); renderer_thread/io `.inspector = bool` messages
   (both message variants already ported).

`widgets/key.zig Event` is PURE data: `input.KeyEvent` + `binding: []Action` + `pty: []u8` +
`imgui_state.selected: bool` (a plain bool — no FFI); `init/deinit/label()` are std-only.
`Inspector` however drags `widgets.surface.Inspector` → `key.Stream` (cimgui `ImGuiTextFilter`),
`terminal_info`, `vt_stream` (termio widget: 811 loc, a full vt-Handler + imports Surface —
i.e. an inspector→core edge, breaks #11/#12), `renderer_info` (needs unported
`renderer.Overlay`). That is a multi-day Phase-3 job.

**RECOMMENDATION (hybrid, minimal type port + deferral):**
- Port NOW into rustty-inspector: `widgets/key.rs` `Event` + `EventRing = CircBuf<Event>` +
  `label()` (deps: rustty-input/vt KeyEvent, rustty-datastruct CircBuf — both ported; dcimgui
  NOT touched) and the `main.zig` re-export `pub use widgets::key::Event as KeyEvent`. ~150 loc,
  0 tests, scoped G3b.
- `Inspector` stays a deferred uninhabited placeholder: `pub enum Inspector {}` in
  rustty-inspector (exact precedent: `renderer::state::Inspector`). Core stores
  `Option<*mut rustty_inspector::Inspector>`; the single `renderer_state.inspector = ...`
  assignment casts to `*mut renderer::state::Inspector` (documented opaque-pointer cast between
  two uninhabited placeholders; renderer must NOT dep inspector — Phase-3 inspector needs
  renderer types, so the edge must point inspector→renderer).
- `activateInspector`/`deactivateInspector`: verbatim-comment deferral + `unimplemented!()`
  (~45 loc, 0 tests — they `alloc.create(Inspector)`, unconstructible until Phase 3). The
  keyCallback defer-block ports REAL with the None-arm live (`ev.deinit`) and the Some-arm as
  `unreachable!()` + verbatim `recordKeyEvent` comment (inspector is always None until Phase 3;
  `insp_ev` is genuinely `Some` only when an inspector exists).
- Result: rustty-inspector stays Phase-3 for ALL its logic; core's input path is 100% real.

---

## 5. Renderer/GPU-coupled paths + xev (D6) map

**Dependency placeholders, not code deferrals.** Two placeholder groups, both core-local in
`rustty-core/src/renderer_seam.rs` (precedent: termio's crate-local xev_seam; avoids committing
rustty-renderer's public GPU API prematurely and dodges TWO Cargo cycles — renderer/Thread.zig
imports configpkg AND core's App.Mailbox, neither of which rustty-renderer may dep):

```rust
/// renderer.Renderer (generic.zig, Metal/OpenGL) — Phase-3 R-d. Inhabited placeholder,
/// methods = unimplemented!() (dependency placeholder, no reference logic stubbed).
pub struct Renderer;
impl Renderer {
    pub fn surfaceInit<S>(rt_surface: *mut S) -> Result<(), RendererError>;      // Surface.zig:493
    /// [ADV-FIX 3] the reference init takes (alloc, options) — keep alloc on the placeholder.
    pub fn init<A: apprt_api::App>(alloc: Allocator, opts: Options<'_, A>) -> Result<Renderer, _>; // :551 (Options: config/font_grid/size/surface_mailbox/rt_surface/thread)
    pub fn finalizeSurfaceInit<S>(&mut self, rt_surface: *mut S) -> ...;         // :712
    pub fn threadEnter<S>(&self, rt_surface: *mut S) -> ...;                     // deinit path :801
    pub fn drawFrame(&mut self, sync: bool) -> ...;                              // draw :884
    pub fn deinit(&mut self);
}
/// renderer.Thread (renderer/Thread.zig field shape, faithful) — Phase-3 R-d + D6.
pub struct Thread<A: apprt_api::App + 'static> {
    pub loop_: xev::Loop, pub wakeup: xev::Async, pub wakeup_c: xev::Completion,
    pub stop: xev::Async, /* render_h/draw_h/draw_now/cursor_h timers + completions */
    pub surface: *mut A::Surface,
    pub renderer: *mut Renderer,
    pub state: *mut rustty_renderer::state::State,
    pub mailbox: *mut rustty_renderer::thread::Mailbox,      // REAL (BlockingQueue<Message,64>, landed)
    pub app_mailbox: crate::app::Mailbox<A>,
    /// [ADV-FIX 2] the landed `thread::DerivedConfig` is `enum {}` (uninhabited) — embedding it
    /// BY VALUE would make Thread<A> and hence Surface<A> uninhabited (references to them are
    /// UB-adjacent; every C4+ test would be forced onto dangling *mut). C0 RETYPES the renderer
    /// thread DerivedConfig placeholder to an INHABITED struct with unimplemented!() methods
    /// (exact xev_seam precedent; consistent with this section's own doctrine).
    pub config: rustty_renderer::thread::DerivedConfig,
    pub flags: Flags,
}
// init/threadMain/deinit = unimplemented!() placeholders.
```

**xev handle unification (prerequisite):** core hands `render_thread.wakeup` (an xev.Async) to
`termio::Options.renderer_wakeup`, which is typed `termio::xev_seam::Async` — the placeholder
types must be SHARED. Create **`rustty-xev` NOW** as the D6 placeholder crate (leaf; move the
type definitions from termio/src/xev_seam.rs verbatim; termio's module becomes
`pub use rustty_xev::*`). This is the crate D6 already names as libxev's future home — filled
in place when the real port lands. (Also add `Clone`/`Copy` to `Async` — Zig copies the handle
at `.renderer_wakeup = render_thread.wakeup`.)

Additive to rustty-renderer: `Message::initChangeConfig<C>(alloc, config: &C)` placeholder
(updateConfig :1782 — generic `C` solely because renderer→config cycles, documented; both
DerivedConfig pointees are already-landed uninhabited placeholders).

**Function-level map** (everything compiles; runtime blocked at the marked leaf only):
- GPU-blocked (Renderer placeholder): `init` (surfaceInit/init/finalizeSurfaceInit), `deinit`
  (threadEnter/deinit), `draw` (drawFrame).
- xev-blocked (Async::notify / Thread::init / threadMain): `queueRender` (:2463 — one line;
  transitively updateConfig/setFontSize/focus flows AT RUNTIME only), `searchCallback_`
  (3× wakeup.notify), `deinit` (renderer stop.notify + io stop.notify), `init`
  (rendererpkg.Thread.init, termio.Thread.init xev parts already landed/deferred, both
  std::thread::spawn wrappers).
- Fully REAL now: renderer_state (State/Preedit/Mouse), renderer_thread.mailbox pushes (7
  sites: inspector ×2, font_grid, change_config, search ×4, crash), size math
  (Size/grid/balancePadding via rustty-render-types), cell/link, the vt search thread
  (spawn + callbacks — vt search::Thread is ported), termio Termio/Exec/StreamHandler, font
  SharedGridSet ref/deref, crash ThreadState, ALL input/mouse/clipboard/selection logic.
- App.zig touches NEITHER GPU nor xev (pure mailbox/dispatch) — 100% portable today.

---

## 6. Chunked execution plan (skeleton → fill; per-chunk scoped-G3b + review)

Line ranges = Surface.zig. Each chunk: build+test workspace green, clippy/fmt, check_acyclic,
scoped G3b missing=0 on the covered range, chunk review before the next.

| # | Content | Ref loc | Est |
|---|---------|---------|-----|
| C0 | Seam prep: rustty-xev crate (move termio xev_seam, re-export, +Copy on Async); zigstd guard-less `thread::Mutex` (+2-3 rustty-only unit tests) + retype `renderer::State.mutex` (touches state.rs, termio queueMessage/mailbox); apprt-api growth (§2 methods, §3 Timeout/Size/`Message<'static>`); renderer `Message::initChangeConfig` placeholder; **[ADV-FIX 2] retype renderer `thread::DerivedConfig` to an inhabited struct**; **[ADV-FIX 5] add core's missing Cargo deps: rustty-util, rustty-unicode, rustty-oniguruma (direct `oni` import Surface.zig:24, not via input), rustty-datastruct, rustty-zigstd, rustty-xev** (all leaves/downward-safe); PROTOTYPE the `type Core` self-referential bound (§1 decision gate — RECORD the outcome in this file as a "C0 DECISION" appendix) | — | 1–1.5d |
| C1 | rustty-inspector: widgets/key.rs `Event`+`EventRing`+`label`, `KeyEvent` re-export, `Inspector` placeholder enum. **[ADV-FIX 7] PARTIAL-FILE port: key.zig's ImGui-dependent section (render/imgui widgets) defers with verbatim comment bodies; only the pure-data Event/EventRing/label surface ports live** | ~150 | 0.5d |
| C2 | core `surface_mouse.rs` — full file + the 1 test (13 cases) | 336 | 0.5d |
| C3 | core `renderer_seam.rs` placeholders (§5) | ~200 | 0.5d |
| C4 | core `app.rs` — ALL of App.zig (struct/Message/Mailbox/create..performAllAction/Wasm block) + `#[cfg(test)]` TestRuntime fake + ~8–12 seam tests (§7) | 604 | 1d |
| C5 | Surface skeleton: struct + InputEffect/Search/Mouse/Keyboard + DerivedConfig(+scaledPadding) + module wiring; field-parity audit vs reference (config-cluster lesson) | 1–456 | 1d |
| C6 | init/deinit/close/surfaceMailbox/queueIo/draw/activate+deactivateInspector(deferred verbatim)/needsConfirmQuit | 457–957 | 1d |
| C7 | handleMessage + selectionScrollTick + childExited(+Abnormally) + passwordInput + searchCallback(_) | 958–1523 | 1d |
| C8 | modsChanged/mouseRefreshLinks/updateRendererHealth/updateScrollbar/notifyConfigConditionalState/updateConfig/recomputeInitialSize + setCellSize→preeditCallback | 1524–1866, 2396–2604 | 1d |
| C9 | Text/dumpText(Locked)/hasSelection/selectionString/pwd/resolvePathForOpening/imePoint/clipboardWrite/copySelectionToClipboards/setSelection(AndCopy) | 1867–2395 | 1d |
| C10 | Key tier: keyEventIsBinding/keyCallback/maybeHandleBinding/deactivateAllKeyTables/catchAllIsIgnore/endKeySequence/encodeKey(Opts) + crash setThreadState/clearThreadState un-defer + `impl ThreadStateSurface for Surface<A>` + crashThreadState. **[ADV 6.1] the reference is 13× direct `crash.sentry.thread_state = …; defer … = null` (Surface.zig:2469…4708) — map Zig-defer onto explicit clear (or a small guard) at each site** | 2605–3281, 6014 | 1–1.5d |
| C11 | textCallback/occlusionCallback/focusCallback/refreshCallback/scrollCallback/contentScaleCallback | 3282–3639 | 0.5d |
| C12 | Mouse tier: isMouseReporting/mouseReport/mouseShiftCapture/mouseCaptured/mouseButtonCallback/maybePromptClick | 3640–4278 | 1d |
| C13 | linkAtPos/linkAtPin/mouseModsWithCapture/processLinks/openUrl/osc8URI/mousePressureCallback/cursorPosCallback/colorSchemeCallback/posToViewport/scrollToBottom/hideMouse/showMouse | 4279–4774 | 1d |
| C14a/b | performBindingAction (split ~:5200) + closingAction — highest review-load switch; two sub-chunks, each scoped-G3b | 4775–5650 | 1.5d |
| C15 | writeScreenFile/completeClipboardRequest/startClipboardRequest/completeClipboardPaste/completeClipboardReadOSC52/showDesktopNotification/presentSurface/getProcessInfo | 5651–6036 | 1d |
| C-T | termio un-defers (§3): mailbox::send, queueMessage locked path, surfaceMessageWriter/rendererMessageWriter, + the mutex-only Termio method family; `new_config` lands in core app glue (apprt-api surface.rs verbatim block realized — needs `App::focused_surface` + `Surface::pwd`, both landed by C9) | — | 1d |
| C-F | Finale: DEFERRALS.md updates (activate/deactivateInspector, renderer_seam, D6 list), PORT_PLAN crate-24 corrections, check_test_counts MAP, whole-workspace gates, full-file G3b sweep, final review pass | — | 0.5d |

Order: C0→C1→C2→C3→C4→C5→…→C15→C-T→C-F (C-T may run any time after C0+C7). Total ≈ 14–16
agent-days. 6k loc never lands in one pass.

---

## 7. Test accounting

- **R1 contract: core owes 1 test** — surface_mouse `keyToMouseShape` (13 sub-cases; port as one
  `#[test]` or 13, rust ≥ zig either way).
- **MAP entries (check_test_counts.py):** `core_surface_mouse` (zig `src/surface_mouse.zig` = 1),
  `core_app` (`src/App.zig` = 0), `core_surface` (`src/Surface.zig` = 0) — map all three so the
  tracker proves coverage (config-cluster precedent: every file mapped).
- **Fake runtime decision (none.zig — read it):** apprt/none.zig is 19 loc: `App` has ONLY
  `performIpc` (returns false) and `Surface = struct {}`. Zig's comptime laziness means none
  never satisfies the full runtime surface — it is an IPC-only backend for headless CLI paths.
  It CANNOT implement the apprt-api `App`/`Surface` traits without fabricating
  perform_action/wakeup/getCursorPos/etc. ⇒ porting it now as THE test backend is NOT faithful.
  Verdict: leave none.zig to the apprt/embedded milestone (as already sequenced), and give core
  a **core-local `#[cfg(test)] mod test_runtime`** fake (`TestApp`/`TestSurface` implementing the
  seam traits, recording `perform_action` calls in a `Mutex<Vec<ActionKey>>`, counting
  `wakeup`s) — honest rustty-only test glue, documented as such (not a port).
- **Recommended rustty-only seam tests (in C4, ~8–12):** App create/deinit empty;
  `Mailbox::push` wakes rt_app (wakeup count) and returns queue Size; drainMailbox dispatch incl.
  `.quit` short-circuit (messages left in queue); focusEvent dedup; colorSchemeEvent → sets
  conditional state + `reload_config{soft:true}` recorded, and no-op on same scheme;
  addSurface → `quit_timer stop` recorded / deleteSurface last → `quit_timer start`;
  deleteSurface clears matching focused_surface; needsConfirmQuit false with zero surfaces;
  keyEvent: release→false, unfocused non-global→false, app-scoped leaf → perform_action
  recorded (real Config keybinds); performAllChainedAction continues past failures. Surface<A>
  itself is intentionally untested-by-construction (init spawns pty/renderer/io threads;
  ghostty has zero Surface tests for the same reason) — G3b + review carry it.
- zigstd `thread::Mutex` gets 2–3 rustty-only unit tests (lock/unlock across threads,
  try_lock, the unlock-from-callee pattern).

---

## 8. Risks / ambiguities → resolutions

1. **Self-referential `Core = Surface<A>` bound** may fight the trait solver on recursive
   normalization. Resolution: C0 prototypes it in isolation; fallback is the void*-cast
   `core_ptr()` mirroring the landed `rt_surface_ptr` (semantics identical, one documented
   unsafe cast).
2. **`AppMailbox::push` → `Message<'static>`** touches landed termio call sites. Verified all
   payloads owned; compile gate in C0. R1-safe (assoc types were declared growth placeholders).
3. **Raw pointers in cross-thread queues** (`Message<A>`, `SurfaceMailbox<A>`) need explicit
   `unsafe impl Send` with the reference's threading contract documented (surface pointers are
   stable for surface lifetime; app thread validates via `hasSurface` — the reference comments
   say exactly this). Miri coverage is limited here; keep the memory rule: raw-ptr args, no
   long-lived `&mut` (rust-mut-args-strongly-protected).
   **[ADV-FIX 6] Two additional landmines in the same family:** (a) `SurfaceMailbox<A>` /
   `app::Mailbox<A>` are copied by value in the reference (Surface.zig:549 `.app = app_mailbox`;
   the `.renderer_wakeup` copy) — `#[derive(Clone, Copy)]` on a `Foo<A>` wrongly demands
   `A: Copy`; write MANUAL `impl Clone`/`impl Copy` (no bound on A). (b) the two
   `std::thread::spawn` sites (renderer thread, io thread) capture raw pointers
   (`*mut Thread<A>`, `*mut Termio<…>`) which are `!Send` — use a small documented Send-wrapper
   newtype around each captured pointer with the threading contract stated at the definition.
4. **Guard-less zigstd Mutex** trades RAII for fidelity. Contained: only renderer-state locking
   uses it; every lock/unlock pairs verbatim with a reference line. Condvar-based impl, no new
   deps, no poisoning.
5. **`renderer_thr`/`io_thr`: `std.Thread` undefined-until-spawn → `Option<JoinHandle<()>>`**
   — a shape deviation; document at the field (Zig `undefined` has no Rust equivalent).
6. **DerivedConfig field-type availability audit** (C5): `RepeatableClipboardCodepointMap`,
   `MouseScrollMultiplier`, `KeyRemapSet`, `SharedGridSet::DerivedConfig`, `oni.Regex` links
   (input::Link::oni_regex + non-optional rustty-oniguruma — confirmed present) — run the
   config-cluster field-parity audit (it caught 7 dropped fields last time).
7. **`input.Binding` API surface** used by App.keyEvent/maybeHandleBinding
   (`Set.getEvent`, `GenericLeaf`, `leaf.generic()`, `actionsSlice`, `action.scoped(.app)`)
   — audit rustty-input in C4/C10; any gaps are additive input ports, not seam changes.
8. **`get_title` returning `Option<&CStr>`** borrows backend-owned storage; lifetime tied to
   `&self` is what embedded.zig actually guarantees (pointer into its stored title). Document.
9. **`A: 'static`** bounds propagate from crash's dyn erasure — declare on core types up front
   (backends are plain owned structs; no practical restriction).
10. **Scope creep via C-T** (termio mutex-family un-defers): time-box it; anything beyond
    send/queueMessage/the two writers + mechanically-unblocked methods stays in DEFERRALS.md
    with the xev family (0 tests, R1-safe either way).

---

## C0 DECISION

**`type Core`: OPTION 1 ADOPTED — the self-referential associated-type bound.
Later chunks (C4+) follow it; option 2 (`core_ptr()` void* cast) is NOT used.**

Prototype (C0, 2026-07-02): `crates/rustty-core/src/lib.rs`,
`#[cfg(test)] mod type_core_prototype` — a stand-in `CoreSurface<A>` (field
`rt_surface: *mut A::Surface`) with the §1 impl-block constraint verbatim:

```rust
impl<A> CoreSurface<A>
where
    A: apprt::App + 'static,
    A::Surface: apprt::Surface<App = A, Core = CoreSurface<A>>,
{
    fn core_of(rt: &A::Surface) -> *mut CoreSurface<A> { rt.core() }
    fn rt_app_of(rt: &A::Surface) -> *mut A { rt.rt_app() }
}
```

plus a dummy backend (`TestApp`/`TestSurface` with
`type Core = CoreSurface<TestApp>`) instantiating the bound concretely.

Evidence (rustc 1.93, the workspace toolchain):

- The trait solver ACCEPTS the bound: `cargo build`/`clippy -D warnings` clean,
  no recursion-overflow or normalization diagnostics anywhere in the chain
  `App::Surface: Surface<App = Self>` + the where-clause
  `A::Surface: Surface<App = A, Core = CoreSurface<A>>`.
- The recursive equality NORMALIZES at instantiation: the
  `option1_self_referential_core_bound` test monomorphizes the generic block at
  `A = TestApp` and round-trips real pointers through the TYPED `core()` /
  `rt_app()` edges (1/1 green).
- The only friction encountered was ordinary trait-method scoping (E0599 until
  `use rustty_apprt_api::Surface as ...` is in scope) — not a solver issue.
  Consequence for C4+: bring the `Surface` trait into scope (aliased; core's
  own struct is also named `Surface`).

C0 also landed the seams later chunks assume (details in DEFERRALS.md
"rustty-core C0"): the shared `rustty-xev` crate (termio `xev_seam` types moved
verbatim; `Async: Copy`); `rustty_zigstd::thread::Mutex` (guard-less, §3) with
renderer `State.mutex` + `termio::Mailbox::send` retyped; the §2 apprt-api
`App`/`Surface` growth methods; `AppMailbox` with concrete
`blocking_queue::{Timeout, Size}` + `push(Message<'static>, ...)`;
`thread::DerivedConfig` inhabited ([ADV-FIX 2]) +
`Message::initChangeConfig<C>` placeholder; core's Cargo deps completed
([ADV-FIX 5]).
