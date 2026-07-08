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
