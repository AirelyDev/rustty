# Inspector `Mouse.last_point` Pin-across-reflow Miri harness (R3)

Isolated, **non-FFI** reproduction of the inspector's `Mouse.last_point: ?terminal.Pin`
hazard — PREPORT_INSPECTOR.md **risk R3** ("`Mouse.last_point` raw-aliases pagelist
memory; INVALIDATED by reflow"). Proves under Miri (never hand-reasoned, R5) that the
port's Pin handling is SB/TB-clean: a `Pin` held across a reflow-like `&mut PageList`
must be re-resolved by **address-only walk** (drop/`None` on failure), never
dereferenced while stale.

## What is modelled (faithful to the real code)

- `src/inspector/widgets/surface.zig:203` — `Mouse.last_point: ?terminal.Pin`, the
  persisted inspector-side mouse state.
- `crates/rustty-vt/src/pagelist/mod.rs:630` — `Pin { node: *mut Node, y, x, garbage }`
  is a **non-owning, `Copy`, raw** back-pointer into a PageList `Node`
  (`pagelist/mod.rs:134`). It owns nothing; copying/dropping it never touches `*node`.
- `crates/rustty-vt/src/pagelist/pin.rs:121-152` — `PageList.pointFromPin` walks the
  LIVE list from `getTopLeft(tag).node` following `.next`, and locates the pin's node
  **purely by address comparison** (`core::ptr::eq(node, p.node)`, pin.rs:126,141),
  reading only the pin's `Copy` scalars (`p.x`/`p.y`). It dereferences ONLY the nodes
  reached through the live traversal — **never `p.node` itself**.
- `surface.zig:377-384` (mouseTable) — the null-guarded re-resolution the shipped
  discipline uses: `const p = mouse.last_point orelse …; const pt =
  …pages.pointFromPin(.active, p) orelse …`.

## The load-bearing SB/TB fact certified

Comparing a dangling raw pointer by address (`ptr::eq`) is a pure integer compare — it
does not access freed memory, so it is defined under both Stacked and Tree Borrows. A
reflow-freed pinned node simply never matches a live node ⇒ `pointFromPin` returns
`None`. The stored `Pin` is a **weak handle, re-resolved by address-walk, never a live
borrow** — exactly R3's required discipline. The NEGATIVE candidate shows that the
naive alternative (dereferencing the stale `p.node`) is a use-after-free, so the
address-only discipline is load-bearing, not incidentally green.

## Candidates & results (the evidence)

| candidate | models | SB | TB |
|---|---|---|---|
| `pin_stale_resolves_none` | **shipped**: pin into node #2, reflow frees node #2 (`&mut PageList`), then `point_from_pin(stale)` — address-only walk ⇒ `None`, NO deref of the freed node | **CLEAN** | **CLEAN** |
| `pin_valid_roundtrip` | positive: a pin into a node that SURVIVES the reflow re-resolves to its live coordinate (the walk isn't vacuously always-`None`) | **CLEAN** | **CLEAN** |
| `pin_deref_stale` | NEGATIVE: the naive port that DEREFERENCES `p.node` after the reflow freed it (`(*p.node).rows`) — use-after-free | **UB** | **UB** |

The negative is flagged **at the stale-`p.node` deref itself** ("pointer arithmetic
failed: allocN has been freed, so this pointer is dangling"), NOT at teardown — i.e.
the UB is the pin violation, not an incidental alloc bug.

## Load-bearing findings

1. **`pointFromPin` never dereferences the pin's own node.** The real
   `pagelist/pin.rs` resolver reads `p.node` only as the RHS of `ptr::eq` and reads
   the pin's `Copy` scalar `p.y`/`p.x`; every memory deref is of a node reached
   through the LIVE `first`→`.next` traversal. That is why feeding it a reflow-stale
   pin is sound — the surface.rs port inherits this for free by calling the already
   address-only `PageList::point_from_pin`.
2. **The stored Pin must be treated as a weak/raw handle.** `Mouse.last_point` is a
   plain `Option<Pin>` value; no `&PageList` borrow is held across the reflow. The
   reflow (`&mut PageList`) and the re-resolution (`&PageList`) are disjoint borrows,
   and the `Copy` Pin carries only a raw address between them.
3. **No code fix was required in `pagelist`** — the resolver is already address-only.
   The harness converts the R3 claim from asserted to Miri-proven and certifies that
   surface.rs's `mouseTable` (which calls `pages.point_from_pin(.active, p)` under the
   surface mutex, INV-1) inherits an SB/TB-clean stale-pin path.

## Run

```sh
for c in pin_stale_resolves_none pin_valid_roundtrip pin_deref_stale; do
  echo "== $c =="
  cargo +nightly miri run --quiet -- "$c"                          # Stacked Borrows (default)
  MIRIFLAGS=-Zmiri-tree-borrows cargo +nightly miri run -q -- "$c" # Tree Borrows
done
```

Expected: `pin_stale_resolves_none` / `pin_valid_roundtrip` print `OK` under both;
`pin_deref_stale` aborts with "Undefined Behavior … pointer is dangling" at the
`(*stale.node).rows` deref under both.
