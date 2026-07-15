# Inspector termio `VTHandler.State.events` self-alias Miri harness (R1)

Isolated, **non-FFI** reproduction of the inspector termio widget's
`VTHandler.State.events` **self-alias** hazard — PREPORT_INSPECTOR.md **risk R1**
(the top SB hazard of the whole gui-inspector tier) and the LIFETIMES row
`termio.zig VTHandler.State events *VTEvent.Ring` ("SELF-ALIAS of Stream.events
(sibling field of same Stream); route appends via raw-self helper, prove on
Miri"). Proves under Miri (never hand-reasoned, R5) that the port's append path is
SB/TB-clean, per the [[shaper-hook-sb-clean-interleave]] doctrine.

## What is modelled (faithful to the real code)

- `src/inspector/widgets/termio.zig` — the widget owns
  `Stream { events: VTEvent.Ring, parser_stream: Stream(VTHandler) }` (two SIBLING
  fields). `recordPtyRead` (termio.zig:45-58) wires a **raw alias of its own
  `events` field** into the parser handler, drives the parser, then clears it:
  ```zig
  self.parser_stream.handler.state = .{ .events = &self.events, ... };  // :54
  defer self.parser_stream.handler.state = null;                         // :56
  self.parser_stream.nextSlice(data);                                    // :57
  ```
- Inside `nextSlice`, `VTHandler.vtRaw` (termio.zig:745-803) appends every built
  event **through that alias** (`vt_events.append(ev)`, :784), including the INV-9
  grow-then-evict recovery (`resize` / `deleteOldest` + re-`append`, :784-798).
- So a long-lived `*mut Ring` aliases the sibling `Stream.events` and is
  mutated-through while `Stream.parser_stream` is `&mut`-borrowed by `nextSlice` —
  the doctrine's "long-lived raw alias into a field + interleaved `&mut` to that
  field" shape.

## The load-bearing SB/TB fact certified

The self-alias must be a **raw `*mut Ring`** (mirroring the Zig `*VTEvent.Ring`),
never a Rust `&mut Ring` *reference*, and every append reborrows it narrowly
(`&mut *ptr`) while the parser drive borrows only the **disjoint** sibling
`parser_stream`/`handler`. A raw pointer taken with `&raw mut self.events` sits at
the *base* of that field's borrow stack (SB) / aliases the parent tag (TB), so it
survives `&mut` reborrows of the disjoint sibling — and even of `events` itself.
That is *why* the raw-self routing is sound.

The discipline is **load-bearing**, not incidentally green: the NEGATIVE candidate
holds a `&mut Ring` *reference* alias (the mis-port of `*VTEvent.Ring` as a Rust
reference) across one interleaved parent reborrow of `events` — under SB the
reference is popped off the stack, under TB the foreign write transitions it to
`Disabled` — and the next append through it is UB.

## Candidates & results (the evidence)

| candidate | models | SB | TB |
|---|---|---|---|
| `raw_alias_append_clean` | **shipped**: `&raw mut self.events` (`*mut`) into the handler, drive the parser (disjoint `&mut self.handler`), append through the raw alias | **CLEAN** | **CLEAN** |
| `raw_alias_grow_evict_clean` | **shipped**, INV-9: the append through the raw alias triggers grow-then-evict (`resize`+`delete_oldest`+re-`append`) | **CLEAN** | **CLEAN** |
| `ref_alias_ub` | NEGATIVE: naive `&mut Ring` *reference* alias held across an interleaved parent reborrow of `events`, then appended through the stale reference | **UB** | **UB** |

The negative is flagged **at the through-the-reference append itself** — SB: "tag
does not exist in the borrow stack"; TB: "reborrow … forbidden … transitioned to
Disabled due to a foreign write" — NOT at teardown, i.e. the UB is the alias
violation, not an incidental bug.

## Load-bearing findings (→ the termio.rs port)

1. **The self-alias is a raw `*mut VTEvent.Ring`, never `&mut`.** `termio.rs`
   models `VTHandler.State.events` as `*mut VTEvent::Ring` and, in `vt_raw`, copies
   it out (`let vt_events = state.events;`) and reborrows narrowly per append
   (`unsafe { (*vt_events).append(ev) }`). No `&mut self.events` reference is ever
   materialized and held across the parser drive.
2. **`recordPtyRead` takes the alias with `&raw mut self.events`** and drives
   `self.parser_stream.next_slice(data)` — the drive borrows only the disjoint
   sibling `parser_stream`, so the alias retains provenance for the whole capture
   window (INV-8), and the `defer state = None` clears it after.
3. **The INV-9 grow-then-evict path is also alias-clean** — `resize`,
   `delete_oldest`, and the re-`append` all go through the same raw `*mut`, proven
   by `raw_alias_grow_evict_clean`.

## Run

```sh
# single green gate (both shipped candidates as #[test]s):
cargo +nightly miri test                            # Stacked Borrows (default)
MIRIFLAGS=-Zmiri-tree-borrows cargo +nightly miri test  # Tree Borrows

# observe the negative UB explicitly:
cargo +nightly miri run -- ref_alias_ub
MIRIFLAGS=-Zmiri-tree-borrows cargo +nightly miri run -- ref_alias_ub
```

Expected: `cargo miri test` reports `2 passed` under both border models;
`ref_alias_ub` aborts with an "Undefined Behavior" borrow-violation at the
through-the-reference `bad_ref.append(222)` under both.
