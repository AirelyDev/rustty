# z2d dashed `joinAndCapInitial` aliasing Miri harness

Isolated, **non-FFI** reproduction of the primary SB/TB hazard of the dashed
tessellation port (`crates/rustty-z2d/src/internal/tess/dashed_plotter.rs`,
`joinAndCapInitial`): the Zig `Plotter.InitialPolygon` embeds `result: *Polygon`
aliasing the plotter's own `result`, and the final join / concat-reassign
surgery / `plotOpenJoined(InitialPolygon, ...)` interleave that alias with
`&mut self.result`. See MEMORY "long-lived raw alias into a field interleaved
with &mut to that field".

`Contour` is modelled as an index arena (like the real `polygon::Contour`), so
its `concat`/`plot` moves carry no raw aliasing — the hazard is solely the
self-referential `result`.

Two candidates (Miri only checks the executed path, selected by argv):

- `safe` — the design rustty actually ships: `InitialPolygon` stores **no**
  result pointer; an `InitialView` stitches the plotter's `result` (borrowed
  disjointly) to the initial polygon's own fields and implements the common
  `Plot`/`State` interface. Must be clean under **both** Stacked and Tree
  Borrows.
- `rawalias` — the naive port storing a `*mut Polygon` in the sub-struct and
  using it after an intervening `&mut self.result`. The UB the safe design
  avoids; Miri must **reject** it.

Run (from this dir):

```sh
# safe: clean under both models
cargo +nightly miri run -q -- safe
MIRIFLAGS=-Zmiri-tree-borrows cargo +nightly miri run -q -- safe

# rawalias: Miri reports Undefined Behavior (retag from a tag not in the stack)
cargo +nightly miri run -q -- rawalias
```

Result (2026-07): `safe` clean under SB + TB; `rawalias` rejected under SB with
"trying to retag ... but that tag does not exist in the borrow stack", confirming
the hazard is real and the shipped design removes it.
