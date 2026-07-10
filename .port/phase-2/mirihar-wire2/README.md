# mirihar-wire2 — the `list_themes` picker Loop-holds-raw-self aliasing proof

Standalone, non-FFI Miri harness authored for **WIRE2 (cli C5)** — the interactive
`list_themes` fuzzy-finder preview picker (`crates/rustty-cli/src/list_themes.rs`,
`run_preview`). It reproduces the exact borrow shape the picker uses when a
`vaxis.Loop(Event)` holds raw `*mut Tty` / `*mut Vaxis` into the pinned run-scope
locals while the spawned read thread dereferences them for the loop's whole life.

## Why (correcting the design's framing)

`WIRE2_DESIGN.md` §3 argued for a boxed `Preview{tty,vx,draw_state}` struct + a
whole-`&mut Preview` avoidance discipline. The port instead uses the C3b/`loop_::
tests::loop_lifecycle` **separate-run-scope-locals** shape (stable addresses, no
move for the reader's life). But `loop_lifecycle` is Miri-green only because its
read thread returns early and never DEREFERENCES the raw ptrs — so it does not
actually exercise the interleave. This harness makes the reader **deref** the raw
ptrs while the main thread drives the disjoint-field `update`/`draw` free fns, and
was run under **both** Stacked and Tree Borrows.

## The finding

The naive shape
```rust
let mut loop_ = Loop::new(&mut tty, &mut vx);   // &mut → *mut, stored
loop_.start();                                  // reader now derefs *tty / *vx
...
update(&mut vx, &mut tty, &mut st, event);      // reborrow the LOCAL &mut tty
vx.render(tty.writer());                         // ... and again
```
is **Undefined Behavior under Stacked Borrows**: the `&mut tty` reborrow of the
*local* pushes a `Unique` retag that **pops the read thread's stored raw tag off the
borrow stack**, so the reader's next `(*tty).read()` retags from a tag that no longer
exists (`trying to retag ... but that tag does not exist in the borrow stack`). Miri
pinpoints it at the reader's `(*s.tty)` deref, blamed on the main thread's
`update(&mut vx, &mut tty, ...)` `Unique` retag. This is a REAL latent bug in the
first cut of `run_preview`, not a harness artifact.

## The fix (applied to `run_preview`)

Derive the raw ptrs **once** via `&raw mut` (a `SharedReadWrite` place-projection)
and thereafter touch `tty`/`vx` **only through those raw ptrs** with **transient**
`&mut *ptr` reborrows (dropped at each call):
```rust
let tty_ptr: *mut Tty = &raw mut tty;
let vx_ptr: *mut Vaxis = &raw mut vx;
let loop_ = Loop::new(tty_ptr, vx_ptr);
...
update(unsafe { &mut *vx_ptr }, unsafe { &mut *tty_ptr }, &mut st, event)?;
unsafe { (*vx_ptr).render((*tty_ptr).writer()) }?;
```
Now the read thread's stored raw ptr and every main-thread reborrow are **children
of the same `SharedReadWrite` parent**; no `Unique` outlives a call, so nothing pops
the reader's tag. After `loop_.stop()` joins the reader, the locals are touched
directly again (the `deinit` phase — sound, reader dead). This is the
`shaper-hook-sb-clean-interleave` memory discipline (route the mutating call via the
alias's own ptr + narrow access; PROVE on an isolated Miri harness).

## Run

```
cd .port/phase-2/mirihar-wire2
cargo +nightly miri test                          # Stacked Borrows (default)
MIRIFLAGS="-Zmiri-tree-borrows" cargo +nightly miri test   # Tree Borrows
```
Both are **GREEN** with the fix; the pre-fix `Loop::new(&mut tty, &mut vx)` +
local-reborrow shape is **RED** under Stacked Borrows (revert the `run_interleave`
raw-ptr block to reproduce).
