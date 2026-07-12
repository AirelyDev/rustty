# Surface `rt_surface` self-referential-pin Miri harness (I2 / R-E)

Isolated, **non-FFI** reproduction of the `ghostty_surface_new` allocation's
self-referential pin — PREPORT_SWIFT_APP.md **invariant I2** / **risk R-E** ("the
self-referential pin is the sharpest UB trap"). Proves under Miri (never
hand-reasoned, R5) that the shipped boxed + no-move allocation keeps the
`rt_surface` back-pointer valid, and that a post-init MOVE of the Surface is UB.

## What is modelled (faithful to the real code)

- `crates/rustty-apprt/src/embedded.rs` — the embedded container `Surface` holds a
  core `CoreSurface` **by value** (`:889 pub core_surface: CoreSurface`).
- `crates/rustty-core/src/surface.rs:1953` — core `Surface::init` writes
  `core_surface.rt_surface = <ptr to the CONTAINER>` (the `rt_surface` arg threaded
  through `embedded.rs:1185`). After init: `container.core_surface.rt_surface ==
  &container` — a **self-referential back-pointer**.
- `crates/rustty-apprt/src/embedded.rs:530` (`performPreAction` set_title) — an
  action callback reaches the embedded backend **through** `rt_surface`
  (`let rt = surface.rt_surface_ptr(); (*rt).title = …`). This is the deref that
  dangles if the container was moved after init.
- **Shipped defence** (`embedded.rs:467` / `:500`): `newSurface` does
  `Box::into_raw(Box::new(MaybeUninit::uninit()))`, lowers **immediately** to a raw
  `*mut Surface`, inits in place, and hands the raw ptr to the C host; `closeSurface`
  frees via `Box::from_raw(surface as *mut ManuallyDrop<Surface>)` — dealloc WITHOUT
  moving. No movable `Box<Surface>` ever exists after init ⇒ address-stable ⇒ the
  back-pointer never dangles.

Each candidate is one executed path (Miri only checks executed code), selected by
argv[1].

## Candidates & results (the load-bearing evidence)

| candidate | models | SB | TB |
|---|---|---|---|
| `pin_shipped` | **shipped**: box an uninit slot, lower to raw immediately, init self-alias to the final heap addr, drive 8 action callbacks through `rt_surface`, free via `ManuallyDrop` — NEVER moved | **CLEAN** | **CLEAN** |
| `pin_move` | NEGATIVE: after init, relocate the Surface bytes to a fresh alloc + free the original storage, then a **write** callback follows `rt_surface` (== the freed original) | **UB** | **UB** |
| `pin_move_readonly` | NEGATIVE: same move, reached through a **read** callback (`getTitle` via `rt_surface`, surface.rs:2976) | **UB** | **UB** |

Both negatives are flagged **at the `rt_surface` deref itself** (`(*rt).title` in
`set_title_via_backpointer` / `read_title_via_backpointer`, main.rs:83 / :91 —
"pointer arithmetic failed: … has been freed, so this pointer is dangling"), NOT at
cleanup — i.e. the UB is the pin violation, not an incidental alloc bug. The negatives
use a raw `Box::into_raw` / `ptr::read` / single-free sequence precisely so there is
no double-free to mask the intended use-after-free.

## Load-bearing findings

1. **A `Box<Surface>` move alone does NOT break the pin.** `let b2 = b1;` relocates
   only the *pointer*, not the heap pointee, so the self-alias stays valid. To break
   the pin you must relocate the Surface's **bytes** to a new allocation (move the
   pointee out / return-by-value / reallocate) — which is exactly what a naive
   refactor of `newSurface` would do. `pin_move` reproduces that; it is the shape a
   future edit must never introduce.
2. **The shipped no-move discipline is sufficient and load-bearing.** Because
   `newSurface` lowers to a raw ptr at birth and `closeSurface` frees via
   `ManuallyDrop`, no `Box<Surface>` is ever moved after init, so `pin_shipped` is
   CLEAN under both models. If the split were cosmetic, `pin_move` would also be
   clean and the certification false-green — it is not.
3. **No code fix was required** (only a documentation cross-reference at the box site
   `embedded.rs:467` and the export `rustty_surface_new`): the current allocation
   already satisfies the no-move invariant. The harness converts the ADV-FIX 8
   pinning claim from asserted to Miri-proven.

## Run

```sh
for c in pin_shipped pin_move pin_move_readonly; do
  echo "== $c =="
  cargo +nightly miri run --quiet -- "$c"                          # Stacked Borrows (default)
  MIRIFLAGS=-Zmiri-tree-borrows cargo +nightly miri run -q -- "$c" # Tree Borrows
done
```

Expected: `pin_shipped` prints `OK` under both; `pin_move` / `pin_move_readonly`
abort with an "Undefined Behavior … pointer is dangling" at the `rt_surface` deref
under both.
