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
