# Chunk-3b aliasing Miri harness

Isolated, **non-FFI** reproduction of the exact borrow structure of the GPU R-d
Chunk-3b cell-fill (`crates/rustty-renderer/src/generic.rs`, `mod macos_impl`), used
to PROVE Stacked-Borrows + Tree-Borrows soundness that the real (Metal/CoreText-gated)
test cannot check under Miri. See `../MIRI_EXCEPTIONS.md` (GPU R-d Chunk 3b section).

Each candidate is one executed path (Miri only checks executed code), selected by argv.

Run (from this dir):

```sh
for c in row_current row_shaper_idiom row_rawsibling row_rawself row_surgical \
         row_persistent row_realistic cursor_current cursor_hoist cursor_rawself; do
  echo "== $c =="
  cargo +nightly miri run --quiet -- "$c"                       # Stacked Borrows (default)
  MIRIFLAGS=-Zmiri-tree-borrows cargo +nightly miri run -q -- "$c"  # Tree Borrows
done
```

## Results (the load-bearing evidence)

| candidate | models | SB | TB |
|---|---|---|---|
| `row_current` | the NAIVE `&mut self` shape loop (what the port first shipped) | UB | UB |
| `row_shaper_idiom` | the shaper crate's own `(*sp).shape()` idiom | UB | UB |
| `row_rawsibling` | raw self + shape via a SIBLING `*mut Shaper` | UB | UB |
| `row_rawself` / `row_surgical` / `row_persistent` / `row_realistic` | **shipped**: shape via `run_iter.hooks.shaper`, `add_*` raw-self narrow, narrow `self.field` | **CLEAN** | **CLEAN** |
| `cursor_current` | `add_cursor(&mut self, *const Cursor)` (naive) | UB | clean |
| `cursor_hoist` / `cursor_rawself` | **shipped**: raw-self receiver | CLEAN | CLEAN |

The one clean structure routes `shape()` through the hook's OWN raw pointer (a child
reborrow) and takes the in-loop `add_*` helpers raw-self (narrow field access only). Any
sibling/whole-`Renderer` reborrow of `font_shaper` between hook uses pops the hook tag.

## Chunk-4 candidates (draw_frame + completion trampoline)

Added for GPU_RD_CHUNK4_DESIGN §5.2 (H1 frame-raw-root re-deref across `&mut self`; H2 the
completion trampoline reborrowing `&mut api` + `&self` while the raw roots are live; the
shared-vs-raw render-pass mix). Selected by argv `drawframe_naive` / `drawframe_shipped`.

| candidate | models | SB | TB |
|---|---|---|---|
| `drawframe_naive` | hold `frame` (a `&mut self.swap_chain[i]`-derived raw) across the whole-`&mut self` `upload_background_image`/`update_csu`, then re-deref | **UB** | clean |
| `drawframe_shipped` | **shipped**: single raw root `self_ptr`; re-derive `frame` via `addr_of_mut!((*self_ptr).swap_chain.frames[idx])` AFTER every whole-`&mut self` call; raw-self+raw-texture `sync_atlas_texture`; type-erased `self_ptr` trampoline (`&mut api.present` + `&self frame_completed`) fired while `self_ptr`+`frame` live | **CLEAN** | **CLEAN** |
| `atlas_self_isolated` | the design §6.3 LITERAL `fn sync_atlas_texture(&self, …, texture: &mut Texture)` — `&self` receiver + a `&mut`-into-`self.swap_chain` texture | **UB** | **UB** |

The `atlas_self_isolated` result is load-bearing: the design §6.3's literal `&self`
signature is UNSOUND because `texture` (`&mut frame.grayscale`) aliases into `self`, so
the `&self` receiver FREEZES the texture bytes and the `*texture` write then pops that
frozen tag (UB under BOTH SB and TB). The shipped `sync_atlas_texture` therefore takes
**raw self + raw texture** — which is ALSO the more faithful port of the Zig
`syncAtlasTexture(self: *const Self, atlas: *const font.Atlas, texture: *Texture)` (all raw).

The load-bearing rules the run proved: (a) `swap_chain.frames` is a fixed ARRAY (place-
projected `[idx]`), not a `Vec` (whose `Index` autorefs a `&mut Vec` that pops the frame
tag); (b) a held `frame` raw root is popped by ANY whole-`&mut *self_ptr` reborrow (the
whole-`&mut self` methods), so it is re-derived fresh after them; (c) `sync_atlas_texture`
takes raw self + raw texture (the texture aliases into `self.swap_chain`, so a `&self`
receiver + `&mut Texture` arg would freeze-then-mutate the same bytes); (d) the trampoline's
`&self frame_completed` whole-shared reborrow is clean because `frame` is never used after
`complete()` (the reference's last frame use is `frame_ctx.complete`).

## SEARCH_BINDING_ARM candidates (construct-then-spawn-then-produce + callback)

Added for SEARCH_BINDING_ARM_DESIGN §3.4 — the Surface `.search` arm's DETACHED
store-then-spawn-then-join-LATER lifetime, which is NOT the GPU cell-fill / draw-frame
shape. Unlike the green vt `search_thread` test (which uses `std::thread::scope` = join
BEFORE the borrow ends), these model a STORED `JoinHandle` joined at `deinit`, with the
main thread producing to the running thread AFTER spawn returns and the search thread
re-entering the surface via a `searchCallback`. A deterministic mpsc handshake sequences
the two threads' borrow ops (channels give happens-before, so there is no data race to
mask the tag verdict). Selected by argv `search_naive` / `search_shipped` /
`callback_naive` / `callback_shipped`.

| candidate | models | SB | TB |
|---|---|---|---|
| `search_naive` | MAIN→child producer via a whole-`&mut Search` reborrow (`self.search.as_mut().unwrap().state.mailbox…`) reaching `st` while the child holds its `thread_main(&mut self)` `&mut st` | **UB** | **UB** |
| `search_shipped` | **shipped**: producer via a decoupled `*const Mailbox` (Box pointee) DERIVED FROM THE POST-MOVE `state_ptr` LINEAGE; `thread: None` placeholder; handle stored + joined LATER at deinit | **CLEAN** | **CLEAN** |
| `callback_naive` | child→MAIN re-entry via `unsafe { &mut *self_ }.searchCallback_(event)` — a whole `&mut Surface` reborrow — concurrent with the main thread's held `&mut self` (performBindingAction) | **UB** | **UB** |
| `callback_shipped` | **shipped**: `searchCallback_(self_: *mut Surface)` — narrow raw field projections only (`(*sp).mailbox` external push + `&(*sp).wakeup` narrow shared notify), never a whole `&mut Surface`/`&Surface`, while main holds `&mut self` | **CLEAN** | **CLEAN** |

**Load-bearing finding (corrects design §2.4):** the decoupled `*const Mailbox` must be
derived from the POST-MOVE `state_ptr` lineage (`&raw const *(*state_ptr).mailbox`), NOT
captured off the pre-move local (`addr_of!(*state.mailbox)`). `Mailbox` is a `Box`, which
asserts noalias: a pointer carrying the PRE-MOVE Box provenance is popped when the search
thread later derefs `self.mailbox` through its held `thread_main(&mut self)` `&mut self`
(the Box re-asserts its own provenance), so the stale alias is use-after-invalidation
under BOTH SB and TB. Deriving the decoupled pointer from the same `state_ptr` the child
derefs through makes the two compatible raw siblings — CLEAN under both. The core arm was
implemented to this corrected lineage; §2.4's `addr_of!(*state.mailbox)` would ship SB+TB
UB. The `callback_shipped` result independently confirms §3.3a: a narrow `&(*sp).wakeup`
shared notify + a `*const` external mailbox push, taken fresh per callback, do NOT pop the
main thread's held `&mut self` (whereas the naive whole-`&mut Surface` reborrow does).

Run:

```sh
for c in search_naive search_shipped callback_naive callback_shipped; do
  echo "== $c =="
  cargo +nightly miri run --quiet -- "$c"                          # Stacked Borrows
  MIRIFLAGS=-Zmiri-tree-borrows cargo +nightly miri run -q -- "$c" # Tree Borrows
done
```

## RENDERTHREAD candidates (W3 §4: renderer-Thread construct-then-spawn + xev callback re-entry)

Added for `W3_DESIGN.md` §4 — the macOS-windowing W3 (E2a/E2b) renderer-`Thread`
event loop: a `RThread` stored in a PINNED Box (the reference
`self.renderer_thread` field), lowered to raw `tp: *mut RThread` at birth; the
child runs `thread_main(tp)` HOLDING `tp` for its whole life while the stubbed
xev loop fires callbacks with the SAME raw `tp` (xev userdata contract), each
callback re-derefing `tp` to call `(*renderer).draw_frame()` + drain the external
mailbox + narrow field writes; CONCURRENTLY the MAIN thread produces
(`wakeup.notify` + `mailbox.push`) and at deinit does `stop.notify` — touching
ONLY the Copy `Async` handles + the `*const` mailbox + a narrow `&(*tp).stop`.
Distinct from the SEARCH candidates: this models a child `threadMain` that holds
`self` ACROSS a loop firing callbacks that re-deref `self` (the xev
userdata-reentry shape), not just a one-shot child→main re-entry. A deterministic
mpsc handshake sequences the two threads' borrow ops (happens-before, no data
race to mask the tag verdict). xev is STUBBED (non-FFI): the "loop" is a
channel-driven tick sequence that invokes the stored callback with the raw
userdata. Selected by argv `renderthread_naive` / `renderthread_shipped`.

| candidate | models | SB | TB |
|---|---|---|---|
| `renderthread_naive` | child `thread_main` binds `let t = &mut *tp` (whole) held across the loop; a callback (`wakeup_cb_naive`) reborrows `&mut *tp` (sibling) → pops `t`; `t.field` re-used after → use-after-pop. (Main producer also reborrows `&mut *tp` whole.) | **UB** | **UB** |
| `renderthread_shipped` | **shipped**: `thread_main` + every callback use raw `tp` with NARROW field access only (`(*self_).renderer`, `(*self_).mailbox`, `(*self_).field`, `(*self_).wakeup.notify()`); main uses a Copy `(*tp).wakeup` read + `*const (*tp).mailbox` + a narrow `&(*tp).stop` for `stop.notify()` | **CLEAN** | **CLEAN** |

**Load-bearing finding:** the child's `threadMain` MUST NOT bind a long-lived
whole `&mut *tp` reborrow — any callback that then reborrows `&mut *tp` (a sibling
Unique retag) invalidates the held `t`, so the next `t.field` use is
use-after-pop under BOTH SB (tag not in the borrow stack) and TB (`Disabled`
child-read forbidden). The shipped shape keeps `tp` a raw pointer throughout,
projecting only narrow per-field accesses `(*self_).<field>`, which retag
disjoint byte ranges and never pop each other. The main thread's post-spawn
access is likewise narrowed to Copy handles + a narrow `&(*tp).stop` shared
notify + the `*const` external mailbox — never `&mut renderer_thread` (§2.2
rule 4). This is the structural proof the §2 `Thread<A>` fill must match: if
`renderthread_shipped` is not green under both models, the raw-self discipline in
the core fill is wrong and must not ship.

Run:

```sh
for c in renderthread_naive renderthread_shipped; do
  echo "== $c =="
  cargo +nightly miri run --quiet -- "$c"                          # Stacked Borrows
  MIRIFLAGS=-Zmiri-tree-borrows cargo +nightly miri run -q -- "$c" # Tree Borrows
done
```

## DISPLAYCB candidates (W5 §6: the `Metal::loop_enter` display-callback trampoline)

Added for `W5_DESIGN.md` §6 — the LAST live-windowing seam. The CA `display` method
(main thread, fired during a `needsDisplayOnBoundsChange` commit) drives a display
callback that recovers `*mut Renderer` via container_of (`@fieldParentPtr`) and calls
the RAW-self draw path, CONCURRENTLY with the render thread's own `drawFrame` — both
serialised by `draw_mutex`. The crux W5 must resolve: is a whole-`Renderer` `&mut`
reborrow through the raw ctx sound, or must BOTH concurrent draw callers be raw-self?
A deterministic mpsc handshake parks the render thread INSIDE its draw call (so its
borrow context is live across the display callback's foreign access) and orders the two
field writes (lock/unlock + channel happens-before ⇒ no data race to mask the borrow
verdict). The layer's display-ctx `Cell<*mut c_void>` is main-thread-only (faithful:
both `setDisplayCallback` and CA `display` fire on the main thread), so the container_of
ctx never crosses threads. Selected by argv `displaycb_naive` / `displaycb_shipped`.

| candidate | models | SB | TB |
|---|---|---|---|
| `displaycb_naive` | PRE-FIX: the render thread draws through the `&mut self` `draw_frame` WRAPPER (a protected whole-`Renderer` `&mut` fn arg, renderer_seam.rs:1208 pre-fix), CONCURRENTLY with the raw display callback's `draw_frame_raw` (mutex lock + plain-field write). One protected `&mut` receiver racing a raw foreign draw. | **UB** | **UB** |
| `displaycb_shipped` | **shipped**: BOTH concurrent callers raw-self — render thread via `&raw mut (*(*owner).renderer).0` (a raw place projection, no protector; renderer_seam.rs:1208 post-fix), display callback via the container_of round-trip (`&raw mut (*rp).api` → `byte_sub(offset_of!(FakeRenderer, api))` → `*mut c_void` in a `Cell` → back → deref ALL of Renderer, §2.4). `draw_frame_raw` locks `draw_mutex`, narrow plain-field write + nested completion re-entry, unlocks. No whole-`Renderer` `&mut` protector spans either call. | **CLEAN** | **CLEAN** |

**Load-bearing findings (the actual Miri output):**
- `displaycb_naive` — **SB**: the display callback's `draw_mutex.lock()` (a foreign
  access through the raw ctx) is refused because it "would remove [Unique …] which is
  strongly protected" — the render thread's `&mut self` wrapper argument. **TB**: the
  same foreign read "would cause the protected tag (currently Unique) to become
  Disabled — protected tags must never be Disabled." This is exactly §2.2's claim: the
  protector is established at the wrapper's entry, BEFORE the body reaches `lock()`, so
  the `draw_mutex` cannot rescue it — **mutex or no mutex**. The UB is at the *first*
  foreign touch (the lock), i.e. the plain-field write is never even reached.
- `displaycb_shipped` — CLEAN under both: with BOTH concurrent callers raw-self, no
  whole-`Renderer` `&mut` protector ever exists; the pre-lock access is only a SHARED
  borrow of the `Sync` `draw_mutex` (two threads shared-borrowing it is fine), and every
  field `&mut`/`&` is transient and dropped while the current thread holds the mutex.
  The container_of provenance round-trip (`+offset` then `−offset` on one unbroken
  allocation provenance, ctx stored/loaded as a pointer in a `Cell` — no int round-trip)
  legally derefs ALL of Renderer.

Because `displaycb_naive` models the render-thread `&mut` receiver that §3.4a removes,
this pair certifies the **actually-shipped** shape and proves the raw-self split of BOTH
concurrent draw callers is **load-bearing** (not cosmetic): if the split were cosmetic,
`displaycb_naive` would be CLEAN and the certification false-green.

Run:

```sh
for c in displaycb_naive displaycb_shipped; do
  echo "== $c =="
  cargo +nightly miri run --quiet -- "$c"                          # Stacked Borrows
  MIRIFLAGS=-Zmiri-tree-borrows cargo +nightly miri run -q -- "$c" # Tree Borrows
done
```

## LOOPTHREAD candidates (VAXIS_C3B_DESIGN §8: the vaxis `Loop` read thread)

Added for `VAXIS_C3B_DESIGN.md` §8 — the vaxis Chunk-3b `Loop.zig` read thread
(`crates/rustty-vaxis/src/loop_.rs`). The general construct-then-spawn + userdata-
callback-reentry family is already certified by RENDERTHREAD; the `Loop` has DISTINCT
field mechanics worth a dedicated pin: the produced-to channel is the internally-synced
**Mutex-backed `Queue`** (not renderthread's plain-field mailbox), `should_quit` is an
**`AtomicBool`** (main stores / child loads), the `thread` handle is a **`Cell` mutated
through `&self` by MAIN** while the child holds `*mut Loop` (§4.4), the re-entrant
callback is the **type-erased SIGWINCH winsize callback** (`*mut c_void` context, the
userdata round-trip an `Arc` cannot model), and the queued element is **OWNED** (a
`Vec<u8>` copied at post time, modelling `Key::into_owned` — the child keeps mutating its
per-iteration stack ring AFTER posting; main never observes it, §4.5/§6). A deterministic
mpsc handshake sequences the two threads' borrow ops (happens-before, no data race to mask
the tag verdict); the winsize callback fires on the child while its borrow context is live.
Selected by argv `loopthread_naive` / `loopthread_shipped`.

| candidate | models | SB | TB |
|---|---|---|---|
| `loopthread_naive` | child `ttyRun` binds `let l = &mut *self_ptr` (WHOLE) held across the read loop; the winsize cb reborrows a SIBLING `&mut *self_ptr` (pops `l`) → `l.field` re-used after = use-after-pop; main's stop/nextEvent also reborrow a whole `&mut Loop`. The whole-`&mut` retag spans the ENTIRE struct (incl. the `thread` field at +0x8), so it RACES main's legitimate narrow `thread.set` — Miri flags it at the child's `&mut *self_ptr`. | **UB** | **UB** |
| `loopthread_shipped` | **shipped**: child + winsize-cb use raw `self_ptr` with NARROW field access only (`(*sp).should_quit` atomic load, `&(*sp).queue` external push, `(*sp).tty`/`(*sp).vaxis`/`(*sp).field` narrow); main = narrow `(*sp).queue` pop + `AtomicBool` store + a narrow `Cell` `thread` set/take, never a whole `&mut Loop`; and the posted item **OWNS** its bytes | **CLEAN** | **CLEAN** |

**Load-bearing finding:** the `Loop` read thread MUST NOT bind a long-lived whole
`&mut *self_ptr`. The whole-`&mut` retag is a *write to the entire allocation* — it not
only pops any sibling `&mut *self_ptr` the winsize callback later takes (use-after-pop),
it also collides with the MAIN thread's perfectly-legal narrow `thread` `Cell` write
(`start`/`stop` mutate `thread` through `&self` while the child runs), producing a data
race under BOTH SB and TB. The shipped shape keeps `self_ptr` raw throughout and projects
only narrow per-field accesses that retag disjoint byte ranges: the `thread` field is
touched by MAIN only, `should_quit` is atomic, `queue` is internally `Mutex`-synced, and
the queued `T` owns every byte (no read-thread ring crosses the boundary) — so nothing
races and no tag is popped (CLEAN under both). If `loopthread_shipped` is not green under
both models, the raw-self narrow-projection discipline in §4 is wrong and must not ship.
(The §4.5 borrowed-ring data race is the *documented* second hazard the owned-copy
removes; Miri halts at the first UB — the whole-`&mut` retag race above — so the owned
promotion is modelled by the shipped candidate's owned push rather than a second detected
race.)

Run:

```sh
for c in loopthread_naive loopthread_shipped; do
  echo "== $c =="
  cargo +nightly miri run --quiet -- "$c"                          # Stacked Borrows
  MIRIFLAGS=-Zmiri-tree-borrows cargo +nightly miri run -q -- "$c" # Tree Borrows
done
```
