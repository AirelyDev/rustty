# rustty-vt — Tracked Deferrals (must be completed by the named sub-batch)

Hard tracker for code/tests/comments deferred from an earlier sub-batch because a
dependency wasn't ported yet. Each MUST be completed by its target sub-batch — the
target workflow consumes this file. Nothing here is "dropped" (R1); it is scheduled.

## Deferred to vt-12 (Terminal/Screen cluster)

From **vt-7 (kitty)** — these need `Screen`/`Terminal`/`ImageStorage`:

1. ~~**`kitty/graphics_unicode.rs` — `render_placement()` + `grid()` methods.**~~
   **RESOLVED (vt-12c-kitty).** `render_placement` (coordinate-space / scaled-rect /
   aspect-fit logic) + `grid` (placement lookup by id, else first virtual placement;
   image-size fallback; `CellCountInt` bounds check) are ported verbatim to
   `kitty/graphics_unicode.rs` from `ghostty/src/terminal/kitty/graphics_unicode.zig`,
   carrying all **58 inner comments**. Scoped graphics_unicode.zig G3b → missing=0.
   - **14 ported tests**: 11 placement-iterator (`unicode placement: …`) + 3 render
     (`unicode render placement: dog 4x2 / 2x2 with blank cells / 1x1`), all
     assertion-for-assertion and passing (1380 → 1394 total, no regression).
   - Memory safety: `unicode_placement_single_row_col` + `unicode_render_placement_dog_1x1`
     are Miri-clean (UB=0) in isolation; all 14 pass under ASan.

2. ~~**`kitty/graphics_storage.zig` (1364 loc, 19 tests)** — full file. Needs `Screen`.~~
   **RESOLVED (vt-12c-kitty).** `ImageStorage` (+ `Placement`/`PlacementKey`/
   `Location`/`pixelSize`/`gridSize`/`rect`/`delete`/eviction) is fully ported in
   `kitty/graphics_storage.rs`; all **19 tests** are ported assertion-for-assertion
   and pass (no regression in the 1353 prior). `Screen.kitty_images` is wired to the
   real `ImageStorage` (init `setLimit`/`image_limits`, deinit, reset, and the 4
   `dirty=true` scroll/resize hooks) under the `crate::options::KITTY_GRAPHICS` gate;
   the self-referential `&mut Screen` borrows are made sound via `mem::take`
   move-out/move-back (Miri-clean). Scoped graphics_storage.zig G3b → missing=0.

3. ~~**`kitty/graphics_exec.zig` (575 loc, 8 tests)** — full file. Needs `Terminal`.~~
   **RESOLVED (vt-12c-kitty).** `execute` (+ `query`/`transmit`/`display`/`delete`/
   `loadAndAddImage`/`encodeError`) is ported verbatim to `kitty/graphics_exec.rs`;
   all **8 tests** are ported assertion-for-assertion and pass (no regression in the
   1372 prior → 1380 total). `Terminal::kitty_graphics` forwards to `execute`, and
   `set_kitty_graphics_size_limit`/`set_kitty_graphics_loading_limits` iterate
   `screens.all` under the `crate::options::KITTY_GRAPHICS` gate (move-out/move-back
   for the storage-borrows-screen aliasing). Scoped graphics_exec.zig G3b → missing=0.
   Memory safety: the 2 data-less tests (`a=p` query/display) are Miri-clean; the 6
   that transmit `;`-data cross the simdutf base64 C FFI (Miri can't execute foreign
   functions — R6) and are covered by ASan (8/8 graphics_exec tests, 0 findings).

4. **`kitty/graphics.rs` re-exports** — `ImageStorage` (from `graphics_storage`)
   **RESOLVED (vt-12c-kitty)**: `pub use storage::ImageStorage` restored + exercised
   in the aggregator `ref_all_decls` test. `execute` (from `graphics_exec`)
   **RESOLVED (vt-12c-kitty)**: `pub use exec::execute` restored + exercised in the
   aggregator `ref_all_decls` test (typed `fn` reference).

**vt-12 acceptance gain:** +~41 tests (19+8+14) and graphics_unicode G3b → 0 missing.

From **vt-11b (formatter production + Screen dump)** — **RESOLVED in vt-12c-fmt:**

5. ~~**ALL ~100 `formatter.zig` `test{}` blocks.**~~ **RESOLVED (vt-12c-fmt).**
   All 100 formatter `test{}` blocks are ported as `#[test]`, assertion-for-
   assertion, in `crates/rustty-vt/src/formatter.rs` (`mod tests`). Each builds its
   fixture via `Terminal::init(...)` + `t.vt_stream().next_slice(...)` (now
   available) and asserts the round-trip/output. Span: `"Page …"`, `"PageList …"`,
   `"Screen …"`, `"TerminalFormatter …"`, plus HTML/VT/style/hyperlink/charset/
   codepoint-map variants. Scoped formatter.zig G3b → missing=0.

6. ~~**`TerminalFormatter` (formatter.zig:141).**~~ **RESOLVED (vt-12c-fmt).**
   `TerminalFormatter::{init,format}` are filled (no more `todo!()`): the
   `terminal: *const Terminal` field is restored and `format` composes the
   already-ported `ScreenFormatter`/`PageListFormatter`/`PageFormatter` while
   emitting the `palette`/`modes`/`scrolling_region`/`tabstops`/`pwd`/`keyboard`
   Extra. The 5 `TerminalFormatter`-driven (`"Terminal vt …"`) Screen tests are
   ported alongside it.

**vt-12c-fmt acceptance gain (formatter):** +100 formatter tests (incl. the 5
`Terminal vt …` tests), `TerminalFormatter::{init,format}` → 0 `todo!()`, and
scoped `formatter.zig` G3b → 0 missing.

## Deferred Screen.zig tests (not-yet-ported test bodies) — vt-11/vt-12 cluster

From **vt-11 (Screen)** — 152 of 187 `Screen.zig` `test{}` blocks are ported; the
remaining ~35 are scheduled, not dropped (R1). 23 of these are flagged by G3b as
test-region "missing" (their inner comments come WITH the deferred test body).
These are NOT kitty/TerminalFormatter tests; they are the
`increaseCapacity`/`scroll-above`/`scrollback`/`hyperlink`/`setAttribute`-OOM edge
tests whose production methods ARE ported but whose test fixtures/assertions land
in a later Screen sub-batch (vt-11h)/vt-12. Each carries its inner comments, so its
G3b "missing" inner comments are EXPECTED until the test is ported:

- `Screen: cursorDown to page with insufficient capacity` (15)
- `Screen setAttribute increases capacity when style map is full` (13)
- `Screen: increaseCapacity non-cursor page returns early` (11)
- `Screen: increaseCapacity cursor style ref count preserved` (6)
- `Screen: increaseCapacity cursor with both style and hyperlink preserved` (5)
- `Screen: increaseCapacity cursor hyperlink ref count preserved` (3)
- `Screen setAttribute splits page on OutOfSpace at max styles` (6)
- `Screen: cursorSetHyperlink OOM + URI too large for string alloc` (6)
- `Screen: hyperlink cursor state on resize` (3)
- `Screen: hyperlink reuse` (2)
- `Screen: scrollback various cases` (5)
- `Screen: scrollback doesn't move viewport if not at bottom` (4)
- `Screen: scroll above same page but cursor on previous page last row` (5)
- `Screen: scroll above same page` (4)
- `Screen: scroll above with cursor on non-final row` (4)
- `Screen: scroll above same page but cursor on previous page` (3)
- `Screen: scroll above creates new page` (2)
- `Screen: scroll above no scrollback bottom of page` (1)
- `Screen: scroll down from 0` (1)
- `Screen: scrolling` (3)
- `Screen: scrolling when viewport is pruned` (2)
- `Screen: scrolling with a single-row screen with scrollback` (1)
- `Screen: select replaces existing pins` (1)

**Acceptance gain:** porting these test bodies drives the Screen.zig test-region
G3b "missing" to 0 (only the kitty-ImageStorage/Valgrind production deferrals above
remain, EXPECTED until vt-12).

## Deferred to Dispatch/SIMD phase

From **vt-9 (stream)**:

1. **`stream.rs` — `Stream::next_slice` SIMD fast path (`nextSliceCapped` + the
   `simd.vt.utf8DecodeUntilControlSeq` loop).** Reference:
   `ghostty/src/terminal/stream.zig:471-577` (`nextSlice` SIMD arm +
   `nextSliceCapped`). This is a documented deferred seam gated on
   `options::SIMD` (the reference's `comptime debug or !build_options.simd`):
   `SIMD` defaults to `false` (`cfg!(feature = "simd")`), so the scalar fallback
   — the reference's own non-SIMD arm — runs unconditionally and is fully ported
   and exercised by all 38 stream tests. The SIMD arm is a `todo!()` documented
   in-source at stream.rs:617-618 with a PORT NOTE at stream.rs:599-602. This is
   NOT a comment/logic drop (R1): it lands with the `simd.vt` primitives in the
   Dispatch/SIMD phase, at which point the `nextSliceCapped` body's inner
   comments come with it.

   **Full deferred-SIMD surface** (the ~23 G3b "missing" on stream.zig all map
   here, EXPECTED until the SIMD phase): `nextSliceCapped`, `consumeAllEscapes`,
   `consumeUntilGround`, the `nextSlice` SIMD arm, and the `cp_buf`/max-codepoints
   chunk-size consts + their `@setEvalBranchQuota` test-comptime comments.
   `initAlloc` is NOT part of this seam — it was ported (stream.rs `init_alloc`)
   as part of closing the vt-9 comment-fidelity gap.

## Deferred to rustty-vt-c (C ABI layer)

Embedded C-ABI declarations inside vt `.zig` files belong to the C FFI layer
(`terminal/c` → the separate `rustty-vt-c` crate, PORT_PLAN module 12), not the
pure-VT `rustty-vt` crate:

1. **`stream.zig` `c_union`** (the Action type's `lib.TaggedUnion(...)` C-ABI
   representation, ref stream.zig:228-234) — carries the
   "TODO: Before shipping an ABI-compatible libghostty, verify this." +
   "This was just arbitrarily chosen for now." comments. Deferred to rustty-vt-c
   (it needs the C-ABI tagged-union machinery + the `[16]u64` size verification).
   Watch for more embedded `c_*`/`lib.TaggedUnion`/C-ABI decls in Screen/Terminal.

## Pre-existing deferred seams (lower priority, noted)
- `sgr.rs` local `SepList` — can be retargeted to re-export `parser::SepList` (vt-5
  defined the canonical one). Cosmetic; tracked PORT NOTE in sgr.rs.
- `sys.decode_png` — PNG codec hook intentionally null in the VT crate (faithful;
  Ghostty tolerates null). A real decoder rides a later FFI crate.

## Rule
A sub-batch's verify MUST treat G3b "missing" that maps to an entry here as
EXPECTED (not RED), and MUST NOT add new deferrals without recording them here.

## Deferred: apc cluster (font-blocked) — revisit after rustty-font

`apc.zig` + `apc/glyph/{request,Glossary,execute,glyph}` (67 tests) need
`font/opentype/glyf` (`Glyf` parser, 44 refs in request.zig) + `font/Glyph` +
`datastruct/circ_buf` (the last is ported: rustty-datastruct/circ_buf.rs). BLOCKER:
**rustty-font is a stub AND already depends on rustty-vt**, so a direct `vt→font`
dep would create a crate cycle, and the glyf-decode tests need a real decoder.
RESOLUTION (deferred): port rustty-font's `glyf`/`Glyph` first, then port apc either
(a) in a crate layered above both vt+font, or (b) via a `GlyfDecoder` trait seam in
vt that font fills (the plan's "sys-style hook"). `apc/glyph/response` (12 tests,
leaf, no font) could go early if needed. Terminal (vt-12) must stub its apc usage
until this lands.

## Deferred: vt-12a Terminal/stream_terminal stubs (font-block continuation + C-ABI)

The vt-12a struct-first skeleton (`terminal.rs` <- `Terminal.zig`,
`stream_terminal.rs` <- `stream_terminal.zig`) lands with these stubs, all of
which CONTINUE existing deferrals (no new capability is being dropped):

- **apc_handler ZST** (`stream_terminal::ApcHandler`): the reference
  `apc.Handler` is a no-op ZST here; `deinit` (and the future `start`/`feed`/
  `end`) are stubbed under the apc-cluster deferral above. The `apc_start`/
  `apc_put`/`apc_end` trait arms and `apcEnd` dispatch are `todo!()` and remain
  no-op once vt-12b-7 fills them, until rustty-font unblocks apc.
- **GlyphGlossary / GlyphRequest / GlyphResponse ZSTs** (`terminal.rs`): the
  reference `glyph.Glossary` / `glyph.Request` / `glyph.Response` are ZST stubs;
  `Terminal::glyph_protocol` is `todo!()` and will be a no-op stub in vt-12b
  (`glyph.execute` is font-blocked). Reactivated with the apc cluster.
- **kitty graphics**: `Terminal::kitty_graphics` forwards to `Screen.kitty_images`
  (a deferred `ImageStorage` stub); reactivated in vt-12c-kitty (graphics_exec
  needs Terminal✓ which now exists). Feature-gated, no new deferral.
- **ScrollViewport C-ABI → rustty-vt-c** (NEW deferral): the reference
  `ScrollViewport = union(Tag)` carries a `lib.TaggedUnion` C-ABI surface
  (`C` / `CValue` / `cval`). In pure-VT the port is a plain Rust enum
  (`Top` / `Bottom` / `Delta(isize)`); the C-ABI tagged-union representation is
  hoisted into the `rustty-vt-c` crate alongside the other `lib.TaggedUnion`
  C surfaces, and is deferred until that crate is carved. No VT behavior is lost
  (the plain enum is the source of truth; the C-ABI is a pure FFI projection).
  G3b for `ScrollViewport.C`/`.CValue`/`.cval` maps here and is EXPECTED-missing.
