# WS10 T20 pty-pipeline aliasing Miri harness

Isolated, **non-FFI** reproduction of the borrow structure of the two-stage pty
read pipeline (`crates/rustty-termio/src/exec.rs`, `ReadThread` — upstream
`2f0e6659d` + `bb0ac4c72`), used to PROVE Stacked-Borrows + Tree-Borrows
soundness that the real (`openpty`/`poll`/`read`-driven) KATs cannot check under
Miri. See `../MIRI_EXCEPTIONS.md`, "rustty-termio B3 — WS10 T20 amendment".

## What is being proven

Upstream states the ring's invariant as a comment:

> A buffer is owned by exactly one stage at a time, so buffer contents need no
> locking. Only the ring metadata is guarded by the mutex.

In Zig that is a comment. In Rust it is an **aliasing claim** about one
allocation with two live threads inside it: the gather stage writes slot `head`
while the parse stage reads slot `tail`, concurrently, out of the same `bufs`.
That claim is the whole risk of the task, so it is proven, not asserted.

The harness models exactly it: two real threads, the same mutex + `batch_ready`/
`slot_free` condvars + `head`/`tail`/`count`/`lens`/`done` protocol, a fake
producer and consumer, and **no `poll`/`read`/fd of any kind**. Each candidate is
one executed path (Miri only checks executed code), selected by **argv** —
deliberately not an env var, because Miri does not forward the host environment,
so an env switch would silently collapse every candidate onto the default and
make the negative controls vacuous.

Every candidate also asserts completeness (every produced byte consumed exactly
once, checksum matched), so a candidate cannot be "clean" by dropping data.

## Run (from this dir)

```sh
for c in shipped naive_mutslice naive_copyfield naive_wholeref; do
  echo "== $c =="
  cargo +nightly miri run --quiet -- "$c"                           # Stacked Borrows (default)
  MIRIFLAGS=-Zmiri-tree-borrows cargo +nightly miri run -q -- "$c"  # Tree Borrows
done
```

## Results (the load-bearing evidence)

| candidate | models | SB | TB |
|---|---|---|---|
| `shipped` | **shipped**: raw-pointer slot handoff via `Pipeline::slot_ptr` | **CLEAN** | **CLEAN** |
| `naive_mutslice` | per-slot `&mut (*bufs)[i]` reborrow | clean | clean |
| `naive_copyfield` | the "obvious cure": hold no reference, copy the bytes out | **UB** — data race | **UB** — data race |
| `naive_wholeref` | the literal transliteration: `&mut *bufs`, then index | **UB** — "tag does not exist in the borrow stack" | **UB** — "write access forbidden" |

Two findings worth stating plainly:

1. **`naive_wholeref` is what a careful reader of the Zig naturally writes.**
   `break :buf &pipeline.bufs[pipeline.head]` transliterates to
   `let bufs = unsafe { &mut *p.bufs.get() }; let buf = &mut bufs[slot];`, whose
   `&mut` tag spans every slot — including the one the other stage is
   concurrently using. UB under both models. The shipped `slot_ptr` discipline is
   therefore load-bearing, not defensive styling.

2. **`naive_mutslice` is CLEAN, and that is a real result, not a hole.** A
   per-element `&mut` reborrow taken through the raw pointer is narrow enough for
   both models. The hazard is specifically the WHOLE-OBJECT reference or copy —
   which is also why `naive_copyfield` fails: a by-value read of the storage is
   still a foreign *access* on the owning allocation (the standing
   `rust-mut-args-strongly-protected` finding). The shipped code uses `slot_ptr`
   anyway, because the narrower shape costs nothing and does not depend on that
   distinction continuing to hold in a future aliasing model.
