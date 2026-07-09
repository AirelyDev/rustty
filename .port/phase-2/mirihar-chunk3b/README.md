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
