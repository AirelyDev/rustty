# SEARCH_BINDING_ARM_DESIGN — filling `performBindingAction .search`

Design for landing the last unported first-half `performBindingAction` switch
arm — `Surface.zig:4933` `.search` — plus its rustty-vt prerequisite (the
search-thread mutex-type unification). **Design only; no `crates/` edits here.**

Reference (read-only): `/Users/gael/projects/ghostty` (Zig 0.15.2) —
`src/Surface.zig` `performBindingAction` `.search` arm and `src/terminal/search/Thread.zig`.

Rules: PORT_RULES R1 (100% fidelity — verbatim body ported, not deleted), R4
(acyclic — no new crate edges), R5 (genuinely-blocked stays `unimplemented!()`).
Memory: `rust-mut-args-strongly-protected`, `intrusive-ptr-and-placeholder-fill`,
`per-subbatch-scoped-g3b`.

---

## 0. Verdict up front

- **`vt_slice = the existing `search::thread::tests::search_thread` test, RETYPED.**
  The rustty-vt search thread is fully ported and the search-thread test already
  exists and is GREEN today (`cargo test -p rustty-vt search::thread::tests::search_thread`
  → `1 passed`). It spawns a real background search over a `Terminal` holding
  `"Hello, world"`, searches `"world"`, and asserts exactly one match at screen
  `x=7..=11`. This IS the "spawn a search over a Screen, assert matches" test the
  directive asks for — it must be UPDATED (not created) when the mutex is retyped,
  and it is the load-bearing regression gate for blocker (a). The vt slice is real
  and testable.

- **The core `.search` arm is COMPILE-faithful.** A full `Surface` is
  windowing-blocked (`Surface::init` panics at `Renderer::surfaceInit`), so the
  arm cannot be driven headlessly and cannot be Miri-checked at the Surface level.
  It lands LIVE (replacing `unimplemented!()`) but its correctness rests on the
  vt-level test (the machinery) + the mirihar harness (the borrow shape), not a
  Surface test.

- **Landing `.search` LIVE is NOT self-contained to the arm.** Once a real search
  thread can be spawned, `self.search` is no longer permanently `None`, so the
  already-LIVE sibling arms (`.navigate_search`, the `.search` re-entry branch)
  and `Search::deinit` will, for the first time, run *against a live thread that
  holds a raw `&mut Thread` alias*. Their current shape — reaching
  `self.search.as_mut().unwrap().state.mailbox` / `.wakeup` / `.stop` through a
  `&mut Search` reborrow — is **Stacked-Borrows UB** under that condition (proven
  shape-equivalent to mirihar `row_current`/`row_shaper_idiom`, both `UB` under
  SB). Landing `.search` therefore REQUIRES retrofitting those producer accesses
  onto decoupled raw handles (§3). It ALSO requires retyping the reverse-direction
  `searchCallback_` off `&mut Surface` (§3.3a) — the search thread's callback forms
  a whole `&mut Surface` reborrow that, once live, races the main thread's `&mut
  self` = the SYMMETRIC SB-UB. Both directions are the substantive part of the
  design and the reason the arm was deferred, not the spawn syntax.

---

## 1. Blocker (a) — search-thread mutex-type unification (rustty-vt)

### 1.1 The mismatch

The reference shares ONE `*std.Thread.Mutex` between the renderer state and the
search thread: `Surface.performBindingAction` passes `.mutex =
self.renderer_state.mutex` into `terminal.search.Thread`'s `Options.mutex`
(`Thread.zig:434 mutex: *Mutex`, `Mutex = std.Thread.Mutex`).

In rustty the renderer `State.mutex` was retyped (core chunk C0) to the
guard-less Zig-shaped mutex and is TODAY:

```rust
// crates/rustty-renderer/src/state.rs:47
pub mutex: *mut rustty_zigstd::thread::Mutex,          // guard-less lock/unlock
```

and the core already stores/locks it that way
(`crates/rustty-core/src/surface.rs`: `Box::into_raw(Box::new(rustty_zigstd::thread::Mutex::new()))`,
`unsafe { (*self.renderer_state.mutex).lock() }`).

But `search::thread::Options.mutex` was ported BEFORE C0 and is still the std
guard mutex:

```rust
// crates/rustty-vt/src/search/thread.rs:840  (CURRENT)
pub mutex: *const std::sync::Mutex<()>,
```

with four internal lock sites using std's RAII guard
(`let _guard = unsafe { (*self.opts.mutex).lock().unwrap() };`). The `.search`
arm cannot compile: it would pass a `*mut rustty_zigstd::thread::Mutex` into a
field typed `*const std::sync::Mutex<()>`.

### 1.2 The retype (concrete)

**Field** (`Options`):

```rust
// crates/rustty-vt/src/search/thread.rs — Options
// mutex: *Mutex,   where  Mutex = std.Thread.Mutex
//
// PORT NOTE: the shared renderer/search mutex is the guard-less Zig-shaped
// `rustty_zigstd::thread::Mutex` (the core C0 mutex-model decision). The raw
// pointer mirrors the reference's shared `*std.Thread.Mutex`; it is locked under
// `unsafe` at the same sites the reference calls `mutex.lock()`.
pub mutex: *const rustty_zigstd::thread::Mutex,
```

`rustty-vt` already depends on `rustty-zigstd` (`Cargo.toml:53`) — **no new crate
edge (R4 clean)**. `Terminal`/renderer types are NOT pulled in; this is a pure
pointer-type swap.

**The four lock sites** (guard → `defer`-style unlock). The guard-less mutex has
no RAII; the reference is `mutex.lock(); defer mutex.unlock();`. Faithful mapping:
a tiny local `defer`-unlock guard (the same `defer`-unlock convention core uses,
e.g. `copy_to_clipboard`), so the existing early-`return`/`?` exits in `select`
stay transparent (behaviour unchanged):

```rust
// additive infra (R1 — necessary glue), local to search/thread.rs:
// realizes `defer self.opts.mutex.unlock()` for the guard-less zigstd Mutex.
struct MutexUnlock<'a>(&'a rustty_zigstd::thread::Mutex);
impl Drop for MutexUnlock<'_> { fn drop(&mut self) { self.0.unlock(); } }
#[inline]
fn lock_deferred(m: *const rustty_zigstd::thread::Mutex) -> MutexUnlock<'static> {
    // SAFETY: `m` is the shared renderer mutex, valid for the thread's lifetime
    // (the reference's `self.opts.mutex`). Locked here; unlocked on guard drop.
    unsafe { (*m).lock(); MutexUnlock(&*m) }
}
```

Each of the four sites — `Thread::thread_main_loop` (the `Search_Tick::Blocked`
arm, ~L418), `Thread::select` (~L520), `Thread::change_needle` (~L659),
`Thread::refresh_callback` (~L776) — changes ONLY the one guard line:

```rust
// BEFORE: let _guard = unsafe { (*self.opts.mutex).lock().unwrap() };
// AFTER:  let _guard = lock_deferred(self.opts.mutex);
```

The bodies (feed/select/etc.) are untouched. The `refresh_callback` site already
hoists `let mutex = self.opts.mutex;` — it passes `mutex` to `lock_deferred`.

> Alternative considered: plain `unsafe { (*m).lock() } … (*m).unlock()` at each
> exit (core's other convention). Rejected for `select`, which has a `?` and two
> `return Ok(())` early exits inside the locked region — the `defer` guard is the
> faithful, exit-safe mapping there, so use it uniformly across the four sites.

### 1.3 Callers / tests updated

The ONLY constructor of `Options` in the tree is the vt `search_thread` test
(`thread.rs:2125`). Update its mutex local + construction; the top-level
`use std::sync::Mutex;` STAYS (the test's `TestUserData` fields
`reset_done`/`selected`/`viewport` are genuine std mutexes and are unrelated to
the shared terminal mutex — do NOT retype those):

```rust
// let mutex: std.Thread.Mutex = .{};
// var t: Terminal = ...
let mutex = rustty_zigstd::thread::Mutex::new();          // was: Mutex::<()>::new()
// ...
Options {
    mutex: &mutex as *const rustty_zigstd::thread::Mutex,  // was: &mutex as *const Mutex<()>
    terminal: &mut t as *mut Terminal,
    event_cb: Some(TestUserData::callback),
    event_userdata: ud_ptr as *mut core::ffi::c_void,
}
```

Nothing else in the test changes (the assertions are on match position, not the
mutex). This is the retyped, still-green **vt slice**.

---

## 2. Blocker (b) — self-referential construct-then-spawn (rustty-core)

### 2.1 The reference shape

```zig
self.search = .{
    .state = try .init(self.alloc, .{ .mutex = self.renderer_state.mutex, ... }),
    .thread = undefined,                        // <-- placeholder
};
const s: *Search = &self.search.?;             // stable ptr into the STORED Search
errdefer s.state.deinit();
s.thread = try .spawn(.{}, terminal.search.Thread.threadMain, .{&s.state});
s.thread.setName("search") catch {};
```

Key: `state` is stored FIRST, then the thread is spawned with `&s.state` — a
stable pointer INTO the now-stored `Search` — then the returned handle is written
back to `s.thread`. `self` (the `*Surface`) is also passed as `event_userdata`,
so the search thread re-enters the Surface via `searchCallback`.

### 2.2 Stability in rustty (no Box needed for the *stability* half)

`Surface` already lives at a **stable heap address** — `init` writes it through a
`*mut Surface` (`std::ptr::write`), and `searchCallback` relies on that (it casts
`event_userdata` back to `*mut Surface`). `Surface.search: Option<Search>` is an
inline field of that pinned Surface, and after the one-time `self.search = Some(..)`
store it is never moved until `deinit`/`take`. So `&self.search.?.state` taken
AFTER the store is a stable address for the thread's whole life — this satisfies
the note's "a field that does not move"; **`Search` need not be `Box`-ed for
stability.** (The mailbox is *already* `Box<Mailbox>` and the loop uses
`Arc<Shared>`, which matters for §3, not for stability.)

### 2.3 `Search.thread` must become `Option<JoinHandle>`

A bare `JoinHandle` cannot be `undefined`; the two-phase store-then-spawn needs a
placeholder. Retype (mirrors the reference `.thread = undefined` then assign — the
exact option the DEFERRALS note floats):

```rust
// crates/rustty-core/src/surface.rs — Search
pub struct Search {
    // state: terminal.search.Thread,
    pub state: terminal::search::thread::Thread,
    // thread: std.Thread,  (`undefined` until the spawn returns)
    pub thread: Option<std::thread::JoinHandle<()>>,

    // -- rustty-only producer handles (see §3) --
    // Decoupled from `state` so post-spawn producers never reborrow `&mut state`.
    mailbox: *const terminal::search::thread::Mailbox, // Box<Mailbox> pointee
    wakeup: terminal::search::thread::xev::Notifier,   // holds Arc<Shared>
    stop: terminal::search::thread::xev::Notifier,     // for deinit's stop-notify
}
```

`Search::deinit` updates the join to `self.thread.take().expect(..).join()` and
routes stop-notify through the decoupled `self.stop` (see §3.3).

### 2.4 The arm body (concrete — replaces the `unimplemented!()`)

```rust
// .search => |text| search: {
binding::ScopedSurface::Search(text) => {
    // const s: *Search = if (self.search) |*s| s else init: { ... };
    if self.search.is_none() {
        // if (text.len == 0) return false;
        if text.is_empty() { return Ok(false); }

        // Stable base address of the pinned Surface (init pins it via *mut Surface,
        // the same address searchCallback casts back). The event_userdata the
        // thread re-enters through — narrow raw field projections only (§3.3).
        let surface_ptr: *mut Surface = self;

        // We need to assign directly to self.search because we need a stable
        // pointer back to the thread state. (reference comment, verbatim)
        // self.search = .{ .state = try .init(...), .thread = undefined };
        //
        // ONE-SHOT BUILD (folded from risk #3): build `state` as a LOCAL, arm its
        // waits + capture the decoupled producer handles OFF the local, THEN move
        // it whole into `self.search` in a single `Search { .. }` literal. This
        // drops the null-mailbox / `Default::default()` placeholders entirely —
        // `xev::Notifier` is `#[derive(Clone)]`-only with no `Default` (thread.rs
        // ~1932), and a two-phase build would need one. The Box<Mailbox> pointee
        // and the notifiers' `Arc<Shared>` are heap-backed, so the `*const Mailbox`
        // and `Notifier`s captured from the local survive the move of `state`.
        //
        // Thread::init returns `Result<_, terminal::search::thread::xev::Error>`
        // (the vt search-loop shim's Error, thread.rs:1592) — a DIFFERENT type
        // from `rustty_xev::Error`. Surface `Error` has NO `From` for it today
        // (only `From<rustty_xev::Error>`/`From<rustty_xev::NotifyError>`), so the
        // `?` below REQUIRES a new additive `impl From<terminal::search::thread::
        // xev::Error> for Error` (variant `Error::Xev`-adjacent; no new crate edge).
        let mut state = terminal::search::thread::Thread::init(
            self.alloc,
            terminal::search::thread::Options {
                mutex: self.renderer_state.mutex,          // now type-compatible (§1)
                terminal: self.renderer_state.terminal,
                event_cb: Some(Self::searchCallback),
                event_userdata: surface_ptr as *mut core::ffi::c_void,
            },
        )?;                                                // needs From<..xev::Error>

        // Arm the wakeup/stop waits ON THE LOCAL + capture the decoupled producer
        // handles BEFORE the move — identical to the vt search_thread test's setup,
        // so the notifiers carry the loop's Arc<Shared> and the mailbox raw is the
        // stable Box pointee (both live OUTSIDE the Thread struct's SB coverage).
        state.wakeup.wait(&state.loop_, &mut state.wakeup_c,
                          terminal::search::thread::xev::Callback::Wakeup);
        state.stop.wait(&state.loop_, &mut state.stop_c,
                        terminal::search::thread::xev::Callback::Stop);
        let mb: *const terminal::search::thread::Mailbox =
            core::ptr::addr_of!(*state.mailbox);           // Box pointee — move-stable
        let wn = state.wakeup.notifier();                  // holds Arc<Shared>
        let sn = state.stop.notifier();

        // self.search = .{ .state = <local>, .thread = undefined };
        // One-shot: move `state` in whole with the captured handles. `thread: None`
        // is the `.thread = undefined` placeholder (§2.3).
        self.search = Some(Search {
            state,
            thread: None,          // == `.thread = undefined`
            mailbox: mb,
            wakeup: wn,
            stop: sn,
        });

        // const s: *Search = &self.search.?;   — ONE transient &mut, taken at
        // birth (nothing aliases `state` yet), lowered immediately to raw. After
        // this line NO &mut Search / &mut Thread is ever formed again.
        let search_ptr: *mut Search = &raw mut *self.search.as_mut().unwrap();
        let state_ptr:  *mut terminal::search::thread::Thread =
            unsafe { &raw mut (*search_ptr).state };       // stable stored-state addr

        // errdefer s.state.deinit();   — on spawn failure, deinit + clear.
        // s.thread = try .spawn(.{}, threadMain, .{&s.state});
        // s.thread.setName("search") catch {};
        struct SendState(*mut terminal::search::thread::Thread);
        unsafe impl Send for SendState {}
        let send = SendState(state_ptr);
        let spawned = std::thread::Builder::new()
            .name("search".into())               // == setName("search")
            .spawn(move || { let s = send; unsafe { (*s.0).thread_main(); } });
        let handle = match spawned {
            Ok(h) => h,
            Err(e) => {                           // errdefer s.state.deinit(); + `try` unwinds
                // FIDELITY (folded): the reference `s.thread = try .spawn(...)`
                // PROPAGATES the spawn error up performBindingAction's `!bool`;
                // `Ok(false)` would SWALLOW it. Run the errdefer-equivalent, then
                // return Err. `Builder::spawn` yields `std::io::Error`, for which
                // surface `Error` already has `From` (surface.rs:1265) — compiles.
                unsafe { (*state_ptr).deinit(); }
                // PORT NOTE (deviation): the reference errdefer runs ONLY
                // `s.state.deinit()`, leaving `self.search = Some(<broken>)` with a
                // deinit'd state + `undefined` thread. In Rust a `Some` holding a
                // deinit'd `Thread` + `thread: None` is a live-but-dangling hazard,
                // so we clear `self.search = None` — a necessary, defensible
                // deviation (the Option is the Rust null; None is the safe empty).
                self.search = None;
                return Err(e.into());             // == the `try` unwinding !bool
            }
        };
        // s.thread = handle;  — write the `thread` field through its OWN raw
        // projection (disjoint sub-place of `search_ptr`); no &mut Search reborrow,
        // so the child's live `state_ptr` tag is NOT popped (SB-clean).
        unsafe { (*search_ptr).thread = Some(handle); }
        // break :init s;  — fall through to the change_needle push below.
    }

    // Zero-length text means stop searching.
    // if (text.len == 0) { s.deinit(); self.search = null; break :search; }
    if text.is_empty() {
        // deinit joins the thread (stop-notify via the decoupled handle), THEN
        // frees state — see §3.3. After take() the child is dead: no alias.
        self.search.take().unwrap().deinit();
        // break :search
    } else {
        // _ = s.state.mailbox.push(.{ .change_needle = try .init(alloc, text) }, .forever);
        // s.state.wakeup.notify() catch {};
        //
        // PRODUCER PATH: reach the mailbox + wakeup through the decoupled handles,
        // NEVER `self.search.as_mut().state` (that would reborrow &mut Thread while
        // the child holds a live &mut self → SB UB; see §3).
        let s = self.search.as_ref().unwrap();          // &Search: reads sibling handles only
        let mailbox: *const terminal::search::thread::Mailbox = s.mailbox;
        let wakeup = s.wakeup.clone();
        let needle = terminal::search::thread::WriteReq::init(text);
        let _ = unsafe { (*mailbox).push(
            terminal::search::thread::Message::ChangeNeedle(needle),
            rustty_datastruct::blocking_queue::Timeout::Forever) };
        let _ = wakeup.notify();
    }
}
```

Verbatim reference body stays as `//` comments above each translated line (R1).

---

## 3. The aliasing discipline (blocker (b), the hard part) — SB-clean by construction

### 3.1 Why the naive shape is UB

Once the child runs `unsafe { (*state_ptr).thread_main() }`, it holds a `&mut self`
(a `&mut Thread`) reborrow of `state_ptr` for its ENTIRE lifetime. Any main-thread
access that forms a `&mut Search` or `&mut Thread` over that same memory — e.g.
`self.search.as_mut().unwrap().state.mailbox.push(..)` — pops the child's tag off
the borrow stack; the child's next use of `state_ptr` is then use-after-invalidation
= **Stacked-Borrows UB**. This is the *identical shape* the mirihar harness already
adjudicated:

| mirihar candidate | our analogue | SB | TB |
|---|---|---|---|
| `row_current` / `row_shaper_idiom` | `self.search.as_mut().state.mailbox…` between hook uses | **UB** | UB / clean |
| `row_rawself` / `row_surgical` | producer via decoupled raw handle, narrow field only | **CLEAN** | CLEAN |
| `cursor_current` | naive `&mut self` + `*const` arg | UB | clean |

SB is the strict bar the port holds (`cursor_current` is TB-clean yet the port
ships the raw-self shape). So the design must be **SB-clean**, not merely TB-clean.

### 3.2 The fix — decoupled producer handles (the vt-test-proven pattern)

The vt `search_thread` test already spawns a live thread and *produces to it from
the main thread SB-cleanly* by never touching the `Thread` struct after spawn:
it captures a `*const Mailbox` (the `Box<Mailbox>` **pointee** — a heap address
OUTSIDE the `Thread` struct's SB coverage) and `Notifier`s (which own the loop's
`Arc<Shared>`, again outside the struct) BEFORE the spawn, then pushes/notifies
through those. `push`/`notify` take `&self`, and the mailbox pointee is only ever
*shared*-borrowed by both threads (`BlockingQueue` is internally synchronized), so
no `&mut` over the child's memory is ever formed.

`Search` therefore stores those same decoupled handles (§2.3):
`mailbox: *const Mailbox`, `wakeup: Notifier`, `stop: Notifier`. Every post-spawn
producer reaches the running thread ONLY through them.

### 3.3 Sibling arms + deinit must be retrofitted (scope of the change)

Because `self.search` becomes `Some` for the first time, these already-LIVE sites
run against a live thread and must move off `&mut state`:

- **`.search` re-entry branch** (§2.4, the `else` push): via `s.mailbox` / `s.wakeup`. ✅ designed above.
- **`.navigate_search`** — currently `self.search.as_mut().unwrap().state.mailbox.push(Select)` + `.state.wakeup.notify()`. Retrofit to the decoupled handles:
  ```rust
  let s = self.search.as_ref().unwrap();      // &Search — no &mut Thread
  let (mailbox, wakeup) = (s.mailbox, s.wakeup.clone());
  let _ = unsafe { (*mailbox).push(Message::Select(sel), Timeout::Forever) };
  let _ = wakeup.notify();
  ```
- **`.end_search`** — `self.search.take()` + `Search::deinit` + apprt notify. `take()`
  removes the Option and `deinit` joins BEFORE freeing, so it is already alias-safe
  once `deinit` itself is fixed (below). No producer reborrow. ✅ (note only).
- **`Search::deinit`** — currently `self.state.stop.notify(); self.thread.join(); self.state.deinit();`.
  The `self.state.stop.notify()` runs WHILE the child is still alive (stop then join)
  → a `&mut Search`(`self`)→`state` reborrow while the child holds `&mut self` = UB.
  Route stop-notify through the decoupled `self.stop` Notifier (which does not touch
  `state`), THEN join (child now dead → no alias), THEN `deinit` the state via the
  now-exclusive `&mut`:
  ```rust
  let _ = self.stop.notify();                 // decoupled — no &mut state
  self.thread.take().expect("spawned").join().unwrap();
  self.state.deinit();                        // child dead: exclusive access OK
  ```

### 3.3a `searchCallback` child→Surface path MUST retype off `&mut Surface` (folded)

The design previously declared `searchCallback`/`searchCallback_` "UNCHANGED". That
is **wrong and unsafe** — landing `.search` ACTIVATES the exact symmetric SB-UB shape
this chunk switches on, in the reverse direction:

- `searchCallback` (surface.rs:3722–3726) does `let self_: *mut Surface = ud.cast();
  … unsafe { &mut *self_ }.searchCallback_(event)` — a whole **`&mut Surface`**
  reborrow — and `searchCallback_(&mut self, …)` (surface.rs:3739) is itself
  `&mut self`. This fires ON THE SEARCH THREAD (on wakeup AND the 40fps refresh
  timer) **while the main thread routinely holds `&mut Surface`** (every
  `performBindingAction` arm — including the `.search`-else / `.navigate_search`
  producer paths — and every input event). Two live `&mut Surface` reborrows of the
  same base pointer = **Stacked-Borrows UB**. The renderer mutex does NOT save it:
  SB is a property of the abstract-machine borrow stack (tag validity), not a data
  race the lock serializes. The reference is SB-immune ONLY because Zig has no `&mut`
  exclusivity — and its own comment ("access ONLY values that never change", takes NO
  lock here) is precisely the contract that lets the Rust port narrow OFF `&mut`.

**Fix — retype `searchCallback_` off `&mut self` to `*mut Surface` + narrow raw field
projections** (the SAME §3 discipline, applied to the callback). Everything it touches
is a never-changing field reachable without a whole-`&mut Surface`:

- `self.renderer_thread.mailbox` — already `unsafe { &*self.renderer_thread.mailbox }`
  (a `*const` deref, no `&mut`); project it from `*mut Surface` directly.
- `self.renderer_thread.wakeup.notify()` — `&self`-receiver; project `&(*sp).renderer_thread.wakeup`.
- `self.surfaceMailbox()` — takes `&mut self` TODAY, but only to coerce `self` into
  `SurfaceMailbox { surface: *mut Surface<A>, app: { rt_app, mailbox: &raw mut (*self.app).mailbox } }`
  (surface.rs:1489, 2632). Since the struct's `surface` field is a `*mut Surface` and
  `app.mailbox` is a raw projection, the callback can build that literal DIRECTLY from
  the raw `self_` (or via a raw-`*mut Surface` variant helper) with NO `&mut Surface`.
- `m.clone()?` / `sel.highlight.clone()?` — element clones; no `self` borrow.

Route `searchCallback` → `searchCallback_(self_: *mut Surface, event)` with narrow
per-field raw projections so **no whole `&mut Surface`/`&Surface` is ever formed while
the main thread holds `&mut self`.** This gives the callback an explicit **SB verdict
(CLEAN)**, not "unchanged". Prove it with the §3.4 `callback_*` mirihar pair (below).

The reference cross-thread Surface access is preserved; only the Rust `&mut`-exclusivity
narrowing changes.

### 3.4 Prove it on `mirihar-chunk3b` before landing (a NEW pattern)

This construct-then-spawn-then-produce shape is NOT one of the existing mirihar
candidates (those cover the GPU cell-fill / draw-frame). Per the directive +
`intrusive-ptr-and-placeholder-fill`, extend `.port/phase-2/mirihar-chunk3b/`
with a new argv-selected candidate pair, structurally reduced (no rustty deps):

- `search_naive`: a `struct S { st: T, mb: Box<Mbx> }` stored in a pinned slot,
  spawn a thread on `&mut st` via a raw `*mut T`, then push via
  `parent.st_field…` (whole-`S` `&mut` reborrow between the child's uses) →
  EXPECT `UB` under `-Zmiri-stacked-borrows` (the `row_current` analogue).
- `search_shipped`: same, but push via a captured `*const Mbx` (the Box pointee)
  + an `Arc`-backed notifier, never reborrowing `&mut st` → EXPECT `CLEAN` under
  BOTH SB and TB (the `row_rawself`/`row_surgical` analogue).

  **The candidate MUST model the DETACHED store-then-spawn-then-join-LATER shape**
  (folded): the `JoinHandle` is STORED in the struct and joined at `deinit` — NOT
  `std::thread::scope`. The GREEN vt `search_thread` test uses `scope` (join-before-
  the-borrow-ends), which does NOT by itself prove the core arm's *detached* lifetime
  where the producer pushes AFTER spawn returns and the handle outlives the arm.
  Miri handles `spawn`+deferred `join` fine, so this is feasible; state it explicitly.
  Structure: `spawn` → return the handle into `S.thread` → push via the decoupled
  `*const Mbx`/notifier from the main thread → later `S.thread.take().join()` →
  `deinit` the state. Assert SB+TB CLEAN across that whole detached window.

- **`callback_*` pair (folded — the child→parent direction, §3.3a):** the symmetric
  shape MUST also be adjudicated, because landing `.search` activates
  `searchCallback`'s cross-thread `&mut Surface` reborrow.
  - `callback_naive`: child thread does `unsafe { &mut *parent_ptr }.method(..)` (a
    whole-`&mut S` reborrow) CONCURRENT with a main-thread whole-`&mut S` →
    EXPECT `UB` under SB (the direct `searchCallback_(&mut self)` analogue).
  - `callback_shipped`: child does narrow raw field projections off `*mut S`
    (never a whole `&mut S`/`&S`) → EXPECT `CLEAN` under BOTH SB and TB (the
    retyped `searchCallback_(self_: *mut Surface)` analogue).

  (If a standing cross-thread-`*mut Surface`-callback precedent already dispositions
  this shape in the harness, cite it instead — but the design MUST carry an explicit
  SB verdict for the callback, not defer it.)

Record the SB/TB verdicts for ALL of `search_naive`/`search_shipped`/`callback_naive`/
`callback_shipped` in the harness README results table (the load-bearing evidence),
exactly as the Chunk-3b/4 rows do. Only after `search_shipped` AND `callback_shipped`
are SB+TB clean does the core arm (+ the retyped callback) land.

---

## 4. Testability & compile boundary

| piece | how it lands | how it's verified |
|---|---|---|
| vt `Options.mutex` retype + 4 lock sites | LIVE | `cargo test -p rustty-vt search::thread::tests::search_thread` (retyped, must stay GREEN) — the real spawn-search-over-a-Screen test |
| vt `lock_deferred` guard | additive infra | exercised by the same test (all four lock sites are on the search path) |
| core `Search` struct (`Option<JoinHandle>` + decoupled handles) | LIVE | `cargo build -p rustty-core` (type-checks the arm + siblings) |
| core `.search` arm body | LIVE, **compile-faithful** | cannot drive (Surface windowing-blocked at `Renderer::surfaceInit`); correctness rests on the vt test (machinery) + mirihar (borrow shape) |
| core `.navigate_search`/`.end_search`/`deinit` retrofit | LIVE | `cargo build -p rustty-core`; behaviour unchanged when `self.search == None` (the pre-existing state) |
| core `searchCallback_` retype off `&mut self` → `*mut Surface` (§3.3a) | LIVE | `cargo build -p rustty-core`; borrow shape via `mirihar-chunk3b` `callback_shipped`, SB + TB clean |
| `impl From<..search::thread::xev::Error> for Error` (§6.5) | additive | `cargo build -p rustty-core` — makes `Thread::init(..)?` compile |
| the construct-then-spawn borrow shape (detached join-later) | — | `mirihar-chunk3b` `search_shipped` candidate, SB + TB clean (§3.4) |

**Compile boundary:** everything type-checks and links; the ONLY thing not
runtime-exercised is the Surface-level `.search` arm itself (windowing-blocked),
and its two risky ingredients are each independently verified elsewhere (the vt
thread test for the machinery, the mirihar harness for the aliasing). No
`unimplemented!()` remains on the `.search` arm.

## 5. Dead-code / no-regression

- No new crate edges (rustty-vt already deps rustty-zigstd; core already deps
  rustty-vt + rustty-zigstd) → **R4 acyclic preserved.**
- The `.search` verbatim reference body is preserved as comments above the
  translation → **R1.** The `unimplemented!()` + its DEFERRALS pointer are removed;
  update `DEFERRALS.md` (the "DEFERRED (within C14a) — `.search` arm" section,
  L1537) to RESOLVED with a pointer here.
- **G3b comment-coverage spans rustty-vt + rustty-core** — the retyped lock sites,
  the new `Search` fields, and the arm body all carry their reference `//` lines;
  run the scoped G3b gate over BOTH crates (per `per-subbatch-scoped-g3b`; do not
  defer coverage to a finale).
- No behavioural change to the vt search logic (pure mutex-type swap; the test
  asserts the SAME match). Sibling arms are behaviour-identical while
  `self.search == None` (their only prior reachable state); their new decoupled
  producer path is only taken once a real thread exists.
- Parity test contract: `std.Thread.Mutex`/zigstd is glue (not in the contract);
  the search-thread test is a ported ghostty test and must stay 1:1 (retyped, not
  dropped) — Search count stays 2==2 per DEFERRALS L1057/L1081.

## 6. Open risks

1. **SB vs TB authority (primary).** The decoupled-handle design is SB-clean by
   the mirihar precedent, but the port has at least one shipped shape that is
   TB-clean-only (`cursor_current`). MUST land `search_shipped` SB+TB clean before
   the core arm; if some unavoidable sub-access is only TB-clean, escalate (do not
   silently ship an SB-UB shape).
2. **`xev::Notifier`/`Mailbox` visibility.** The decoupled handles need
   `search::thread::xev::Notifier`, `Mailbox`, and `Async::notifier()` reachable
   from rustty-core. `Notifier`/`notifier()`/`Mailbox` are already `pub` in the
   seam; confirm the `xev` module + `WriteReq`/`Message`/`Timeout` re-exports are
   `pub` at the paths core uses (the vt test uses them from within the crate — core
   is a downstream crate, so a `pub use` may be needed; additive, no new edge).
3. **`Notifier: Default` — RESOLVED (one-shot build).** `xev::Notifier` is
   `#[derive(Clone)]`-only (verified thread.rs ~1932); it has NO `Default`. §2.4 is
   rewritten to the **one-shot build** — arm the waits + capture `mailbox`/`wakeup`/
   `stop` handles off the LOCAL `state`, then move `state` in whole via a single
   `Search { .. }` literal — so NO placeholder and NO `Default` is needed. (Deriving
   `Default` on `Notifier` — both fields `Option<..>` — remains a trivial fallback,
   but the one-shot is adopted; the fallback is not used.)
4. **`errdefer` fidelity on spawn failure — RESOLVED (propagate `Err`).** The
   reference `s.thread = try .spawn(...)` PROPAGATES the spawn error up
   `performBindingAction`'s `!bool`; `Ok(false)` would SWALLOW it and is NOT
   faithful. §2.4 now runs the errdefer-equivalent `(*state_ptr).deinit()` + clears
   `self.search = None` (a noted deviation — the reference leaves `Some(<broken>)`)
   then `return Err(e.into())`. `Builder::spawn`'s `std::io::Error` already has
   `From` for surface `Error` (surface.rs:1265) → compiles.
5. **`From<..search::thread::xev::Error>` MISSING — NEW required additive impl.**
   `Thread::init` returns `Result<_, terminal::search::thread::xev::Error>` (the vt
   search-loop shim Error, thread.rs:1592), a DIFFERENT type from `rustty_xev::Error`.
   Surface `Error` has `From` for `rustty_xev::Error`/`NotifyError` but NONE for the
   search shim's `xev::Error`, so §2.4's `Thread::init(..)?` will NOT compile as-is.
   Add `impl From<terminal::search::thread::xev::Error> for Error` (additive, no new
   crate edge — core already deps rustty-vt). Its sole variant is `InitFailed`; map
   to the existing `Error::Xev`-family or a dedicated variant.
6. **`searchCallback` child→Surface SB — RESOLVED (retype, §3.3a).** The prior
   "UNCHANGED" was unsafe: landing `.search` activates a cross-thread whole-`&mut
   Surface` reborrow concurrent with the main thread's `&mut self` = SB UB.
   `searchCallback_` is retyped off `&mut self` to `*mut Surface` + narrow raw field
   projections; verdict SB CLEAN, proven by the §3.4 `callback_shipped` mirihar
   candidate. Must be SB+TB clean before landing.
7. **Surface can't Miri-test the arm.** Accepted: the arm is compile-faithful and
   its risks are covered by the vt test + mirihar (both the producer `search_*` and
   the callback `callback_*` pairs). If a headless Surface seam later exists, add a
   driven test then.
