# VAXIS Chunk 2 — The RENDERING layer — DESIGN

**Crate:** existing `crates/rustty-vaxis` (Chunk 1a value types + 1b uucode/gwidth/
unicode already landed, `lib.rs` PORT STATUS `done`). This chunk adds the RENDERING
layer as three new modules — `screen.rs`, `internal_screen.rs`, `window.rs` — wired
into `lib.rs`.

**Source (read-only):**
`/Users/gael/.cache/zig/p/vaxis-0.5.1-BWNV_LosCQAGmCCNOLljCIw6j6-yt53tji6n6rwJ2BhS/src/`
— `Screen.zig` (73 LOC, 1 test), `InternalScreen.zig` (133 LOC, 0 test),
`Window.zig` (889 LOC, 8 test).

**Chunk 2 scope — port faithfully (R1):**
- `Screen.zig` — the flat cell buffer (`init`/`deinit`/`writeCell`/`readCell`/`clear`
  + `width`/`height`/pixel dims + cursor + `width_method`/shapes).
- `InternalScreen.zig` — the allocating back buffer for diffing (`InternalCell` with
  owned grapheme/uri/uri_id byte buffers + `eql`; `init`/`deinit`/`writeCell`/`readCell`).
- `Window.zig` — the window view (`initChild`/`child` + `BorderOptions`, `writeCell`/
  `readCell`, `clear`/`fill`, `gwidth`, cursor methods, `print`/`printSegment`
  [gwidth-aware grapheme-segmentation layout: grapheme/word/none wrap], `scroll`,
  `hasMouse`, `LineIterator`/`WhitespaceTokenizer`).

**Deps consumed (all intra-crate, already present — confirmed):**
- `cell` — `Cell<'a>` (+ `Style`, `Color`, `Segment<'a>`, `Character<'a>`,
  `Hyperlink<'a>`, `Scale`, `CursorShape`). Every byte-slice field is `&'a [u8]`.
- `gwidth` — `gwidth(str: &[u8], method: Method) -> u16`; `Method::{Unicode,Wcwidth,NoZwj}`.
- `unicode` — `grapheme_iterator(str) -> GraphemeIterator`; `Grapheme { start, len }`
  with `.bytes(str) -> &[u8]`; iterator `next() -> Option<Grapheme>`.
- `mouse` — `Mouse` value struct (for `hasMouse`).
- `image` — `Placement` (the `Cell.image` field type only; shape landed in 1a).
- `lib::Winsize` — `{ rows, cols, x_pixel, y_pixel }` (for `Screen::init`).

**Test inventory — 9 `test{}`** (all quoted, all counted by `check_test_counts.zig_count`):

| # | file:line | Zig name | exercises |
|---|---|---|---|
| 1 | Screen.zig:71 | `refAllDecls` | smoke (reify decls) |
| 2 | Window.zig:481 | `Window size set` | `initChild` size clamp |
| 3 | Window.zig:497 | `Window size set too big` | clamp to parent |
| 4 | Window.zig:513 | `Window size set too big with offset` | offset+clamp |
| 5 | Window.zig:529 | `Window size nested offsets` | `x_off`/`y_off` accumulation |
| 6 | Window.zig:545 | `Window offsets` | negative parent-offset clip; null read + no-op write |
| 7 | Window.zig:564 | `print: grapheme` | **c1b gwidth + grapheme wrap** (commit=false) |
| 8 | Window.zig:627 | `print: word` | **c1b gwidth + word wrap + newlines** (commit=false) |
| 9 | Window.zig:887 | `refAllDecls` | smoke (reify decls) |

InternalScreen.zig has **0** tests. Two of the nine are `test "refAllDecls"` — quoted,
so `zig_count` counts them (it skips only *anonymous* `test {` reify blocks). Port each
as a minimal Rust `#[test]` smoke (constructs the type / touches the public decls). See
§8 and §10.

---

## DISPOSITION: **READY**

Every dependency is present and pure. No method in the three files needs a later chunk
(`Vaxis`/`Tty`/`Loop`/`render`) — the whole layer is self-contained over `Screen` +
`gwidth` + `unicode` + `cell`. **There are ZERO new R5 seams inside these three files.**
`Image::draw`/`cellSize` remain the crate's only seams (in `image.rs`, out of C2 scope —
§7). The one structural decision — the `Window → *Screen` back-reference under
child-window aliasing — is resolvable with the codebase's established raw-pointer
discipline (§5). Nothing is blocked. Proceed.

---

## 1. Per-file mapping (Zig → rustty module)

| Zig source | rustty module | new type(s) | lifetime |
|---|---|---|---|
| `Screen.zig` | `screen.rs` | `Screen<'a>` | `'a` (buf holds `Cell<'a>`) |
| `InternalScreen.zig` | `internal_screen.rs` | `InternalScreen`, `InternalCell` | **none** (owns all bytes) |
| `Window.zig` | `window.rs` | `Window<'a>` (+ `ChildOptions`, `BorderOptions`, `PrintOptions`, `PrintResult`, `LineIterator`, `WhitespaceTokenizer`) | `'a` (`screen: *mut Screen<'a>`) |

`lib.rs`: add `pub mod screen; pub mod internal_screen; pub mod window;` and extend the
CHUNK banner / PORT STATUS to note the rendering layer landed.

---

## 2. `screen.rs` (Screen.zig) — the flat cell buffer

**Struct `Screen<'a>`** (Zig field defaults in parens):
`width: u16 (0)`, `height: u16 (0)`, `width_pix: u16 (0)`, `height_pix: u16 (0)`,
`buf: Vec<Cell<'a>> (empty)`, `cursor_row: u16 (0)`, `cursor_col: u16 (0)`,
`cursor_vis: bool (false)`, `width_method: gwidth::Method (Wcwidth)`,
`mouse_shape: mouse::Shape (Default)`, `cursor_shape: cell::CursorShape (Default)`.

- **Ownership (KEY DECISION):** Zig `buf: []Cell` (allocator-owned slice) → **`Vec<Cell<'a>>`**
  (the crate OWNS the flat cell buffer). `Cell<'a>` carries `'a` because
  `Character.grapheme: &'a [u8]`; cells written into the buffer borrow their grapheme
  bytes from caller memory (segment text / `'static` border glyphs) — Zig's exact
  "caller keeps the text alive" contract, made explicit by threading `'a` through
  `Screen<'a>` and `Window<'a>` (§5).
- **`Default`:** implement a **manual `impl Default for Screen<'a>`** (NOT `derive`) —
  the non-zero Zig field-level defaults `width_method = .wcwidth`, `mouse_shape = .default`,
  `cursor_shape = .default` live at the field-declaration site, not as enum-level defaults;
  reproduce them explicitly. (`test "print: grapheme"` builds `Screen { width_method:
  Method::Unicode, ..Default::default() }`.)
- **`init(winsize: Winsize) -> Screen<'a>`** — `w = winsize.cols`, `h = winsize.rows`;
  `buf = vec![Cell::default(); (w as usize)*(h as usize)]` (the Zig `alloc` + `@memset`
  base_cell in one — `Cell::default()` IS the Zig `.{}` base cell). Set `width_pix =
  winsize.x_pixel`, `height_pix = winsize.y_pixel`, `width = w`, `height = h`.
  **PORT-NOTE:** Zig `init(alloc, winsize) Allocator.Error!Screen` — drop the allocator
  param and the error union; `Vec` alloc is infallible (aborts on OOM), consistent with
  the crate's pure-leaf model. Preserve the `w`/`h`/`base_cell` locals + `@memset` intent
  in comments.
- **`deinit`** — Zig `alloc.free(self.buf)` → Rust `Drop` of `Vec` (no explicit method
  needed; if a `deinit(&mut self)` shim is kept for parity it just `self.buf.clear()`;
  PORT-NOTE the allocator drop).
- **`writeCell(&mut self, col: u16, row: u16, cell: Cell<'a>)`** — guard
  `if col >= self.width || row >= self.height { return }`; `i = (row as usize)*(self.width
  as usize) + col as usize`; `debug_assert!(i < self.buf.len())`; `self.buf[i] = cell`.
  Preserve the `/// writes a cell to a location. 0 indexed` comment.
- **`readCell(&self, col: u16, row: u16) -> Option<Cell<'a>>`** — mirror guard → `None`;
  `debug_assert!`; `Some(self.buf[i])` (`Cell` is `Copy`).
- **`clear(&mut self)`** — `self.buf.fill(Cell::default())` (Zig `@memset(self.buf, .{})`).

**Bounds semantics (fidelity):** Screen uses `>=` (correct, in-bounds) — DISTINCT from
InternalScreen's `<` quirk (§3). `assert(i < buf.len)` → `debug_assert!` (Zig
`std.debug.assert` is active in Debug/Safe/test, elided in ReleaseFast — `debug_assert!`
matches; tests + Miri run in debug so it is live).

---

## 3. `internal_screen.rs` (InternalScreen.zig) — the diffing back buffer

**`InternalCell`** — the back-buffer cell, OWNING its byte data (no lifetime):
`char: Vec<u8> (owned grapheme)`, `style: cell::Style (default)`, `uri: Vec<u8> (owned)`,
`uri_id: Vec<u8> (owned)`, `skipped: bool (false)`, `default: bool (**true**)`,
`skip: bool (false)`, `scale: cell::Scale (default)`. Preserve the field comments
(`// if we got skipped because of a wide character`, the two-line `skip` comment).

- **KEY DECISION — arena → per-cell `Vec<u8>`:** Zig backs `char`/`uri`/`uri_id`
  (`ArrayListUnmanaged(u8)`) with a per-screen `*ArenaAllocator` (bump-alloc, freed
  wholesale in `deinit`). Rust replaces the arena with **plain owned `Vec<u8>` per field** —
  each `InternalCell` owns its bytes; `deinit` becomes `Drop`. This is the faithful
  ownership model (the arena was purely an allocation strategy; semantically each cell
  owns copies of the grapheme/uri bytes that `writeCell` `appendSlice`s in). **No
  lifetime parameter** on `InternalScreen`/`InternalCell` — they hold no borrows.
  **PORT-NOTE** the arena removal prominently; drop the `arena: *ArenaAllocator` field.
- **`default: bool = true` quirk:** note it — the back-buffer cell defaults `default=true`
  (a fresh diff cell is "the default cell"), UNLIKE `Cell.default = false`. `eql`'s
  fast-path depends on it. Preserve.
- **`InternalCell::eql(&self, cell: &Cell) -> bool`** — port assertion-for-assertion:
  1. fast-path `if self.default && cell.default { return true }` (+ its comment
     `// fastpath when both cells are default`);
  2. `std.mem.eql(u8, self.char.items, cell.char.grapheme)` → `self.char == cell.char.grapheme`
     (i.e. `self.char.as_slice() == cell.char.grapheme`), **and** `Style::eql(self.style,
     cell.style)`, **and** `self.uri == cell.link.uri`, **and** `self.uri_id ==
     cell.link.params`. Byte-slice equality throughout — never `String`.

**`InternalScreen`** fields: `width: u16 (0)`, `height: u16 (0)`, `buf: Vec<InternalCell>`,
`cursor_row/col: u16 (0)`, `cursor_vis: bool (false)`, `cursor_shape: cell::CursorShape
(default)`, `mouse_shape: mouse::Shape (default)`. (No `arena` field — see above.)

- **`init(w: u16, h: u16) -> InternalScreen`** — `buf` of `w*h` cells; for each cell set
  `char = vec![b' ']` (the Zig `initCapacity(…,1)` + `appendAssumeCapacity(' ')`), `uri`/
  `uri_id` empty, other fields default (`default=true`). Preserve `/// sets each cell to
  the default cell`. **PORT-NOTE:** Zig `init(alloc,w,h) !InternalScreen` creates the arena;
  Rust drops the allocator + error union (infallible `Vec`); the per-cell `initCapacity(1)`
  + `appendAssumeCapacity(' ')` → `vec![b' ']`.
- **`deinit`** → `Drop` (Zig `arena.deinit(); destroy(arena); self.* = undefined` → Rust
  drop of `Vec<InternalCell>`; PORT-NOTE `self.* = undefined`).
- **`writeCell(&mut self, col, row, cell: &Cell)`** — **preserve the `<` bounds quirk
  VERBATIM** (do NOT "fix" to `>=`): `if self.width < col { return }` (+ `// column out of
  bounds`), `if self.height < row { return }` (+ `// height out of bounds`). Then
  `i = (row as usize)*(self.width as usize) + col as usize`; `debug_assert!(i < buf.len)`;
  `self.buf[i].char.clear(); self.buf[i].char.extend_from_slice(cell.char.grapheme);`
  same for `uri`/`uri_id` (from `cell.link.uri`/`cell.link.params`); `self.buf[i].style =
  cell.style; self.buf[i].default = cell.default`.
  **PORT-NOTE:** Zig wraps each `appendSlice` in `catch { log.warn(...) }`. Rust `Vec`
  alloc is infallible → no error path; drop the `log = std.log.scoped(.vaxis)` import and
  the three `catch`/`log.warn` blocks, but preserve their intent as a PORT-NOTE (the OOM
  branch is unreachable in Rust). `clearRetainingCapacity` → `Vec::clear` (retains
  capacity — exact).
- **`readCell(&self, col, row) -> Option<Cell<'_>>`** — same `<` guards → `None`; note Zig
  here uses `(row * self.width) + col` (u16 arithmetic, no `@intCast(usize)` — a second
  quirk vs `writeCell`; the index is then used as `usize`). Reproduce as `let i = (row as
  usize) * (self.width as usize) + col as usize;` (value-identical for in-range inputs).
  `debug_assert!`; return a borrowed `Cell` whose `char.grapheme = &self.buf[i].char`,
  `link.uri = &self.buf[i].uri`, `link.params = &self.buf[i].uri_id`, `style`, `default`
  (the other `Cell` fields take their `Default`). **The returned `Cell<'_>` borrows from
  `&self`** — the lifetime ties the read cell to the back buffer (faithful to Zig's
  `.grapheme = cell.char.items` aliasing the arena).

---

## 4. `window.rs` (Window.zig) — the window view

### 4.1 Struct + the `screen` back-reference (KEY DECISION — see §5)

**`Window<'a>`** (Zig has NO field defaults — every field is set at each construction site;
do not add `Default`):
`x_off: i32`, `y_off: i32`, `parent_x_off: i32`, `parent_y_off: i32` (all Zig **`i17`** →
Rust **`i32`**, PORT-NOTE the widening — Rust has no `i17`; `i32` covers the range and the
clip math stays exact), `width: u16`, `height: u16`, `screen: *mut Screen<'a>` (Zig
`*Screen`). Preserve all four offset doc-comments (`absolute horizontal offset…`,
`relative horizontal offset, from parent window. This only accumulates if it is negative
so that we can clip the window correctly`, etc.) and the `width`/`height` comments
(`This can't be larger than the terminal screen`).

**`#[derive(Clone, Copy)]`** — Zig `Window` is a value type passed by-value (`self: Window`,
never `*Window`) and `child()` copies it; the raw pointer keeps it `Copy` (§5).

### 4.2 `initChild` + `child` (+ `ChildOptions`, `BorderOptions`)

- **`initChild(self, x_off: i32, y_off: i32, maybe_width: Option<u16>, maybe_height:
  Option<u16>) -> Window<'a>`** (private `fn`). Preserve the doc-comment (`Creates a new
  window with offset relative to parent…Windows do not retain a reference to their parent
  and are unaware of resizes.`). Port the i17 math in `i32`:
  - `max_height = max(self.height as i32 - y_off, 0)`, `max_width = max(self.width as i32 -
    x_off, 0)`.
  - `width: u16 = maybe_width.unwrap_or(max_width as u16)`,
    `height: u16 = maybe_height.unwrap_or(max_height as u16)`.
  - result: `x_off = x_off + self.x_off`, `y_off = y_off + self.y_off`,
    `parent_x_off = (self.parent_x_off + x_off).min(0)` (the `@min(…, 0)` — only
    accumulates when negative), `parent_y_off` likewise,
    `width = width.min(max_width as u16)`, `height = height.min(max_height as u16)`,
    `screen = self.screen`.
- **`ChildOptions`** — `x_off: i32 (0)`, `y_off: i32 (0)`, `width: Option<u16> (None)`,
  `height: Option<u16> (None)`, `border: BorderOptions (default)`. Preserve the
  `the width/height of the resulting child, including any borders` comments.
- **`BorderOptions`** — `style: cell::Style (default)`, `where: Where (Where::None)`,
  `glyphs: Glyphs (Glyphs::SingleRounded)`. Nested:
  - `Where` = Zig anon `union(enum){ none, all, top, right, bottom, left, other: Locations }`
    → `enum Where { None, All, Top, Right, Bottom, Left, Other(Locations) }`.
  - `Locations` = `packed struct { top, right, bottom, left : bool = false }` →
    `struct Locations { top: bool, right: bool, bottom: bool, left: bool }`
    (`#[derive(Default, Clone, Copy)]`).
  - `Glyphs` = `union(enum){ single_rounded, single_square, custom: [6][]const u8 }` →
    `enum Glyphs<'g> { SingleRounded, SingleSquare, Custom([&'g [u8]; 6]) }`. Preserve the
    `custom border glyphs` doc-block ([0]=top left … [5]=bottom left).
  - Module consts `single_rounded: [6][]const u8 = { "╭","─","╮","│","╯","╰" }` and
    `single_square = { "┌","─","┐","│","┘","└" }` → `const SINGLE_ROUNDED: [&[u8]; 6]` /
    `SINGLE_SQUARE: [&[u8]; 6]` of the box-drawing **byte strings** (UTF-8 bytes, never
    `String`).
- **`child(self, opts: ChildOptions) -> Window<'a>`** — port faithfully (`/// create a
  child window`): `initChild` → resolve `glyphs` (rounded/square consts or the custom
  array) → build the six `Character` corner/edge glyphs (`{ grapheme: g, width: 1 }`) →
  `match opts.border.where` (the `.none => return result` early return; `.all`/single-side
  → the corresponding `Locations`; `.other(loc) => loc`) → draw top/bottom/left/right runs
  with `result.write_cell(...)` (`h -| 1` saturating on the bottom/right — see §6 on
  `-|`) → draw the four corners under `loc.top && loc.left` etc. → compute
  `x_off`/`y_off`/`h_delt`/`w_delt`/`h_ch`/`w_ch` (all `-|` saturating) → return
  `result.initChild(x_off as i32, y_off as i32, Some(w_ch), Some(h_ch))`. Preserve the
  `// draw corners` comment.

### 4.3 Cell / cursor / fill methods

- **`writeCell(self, col: u16, row: u16, cell: Cell<'a>)`** — the 6-way guard
  (`self.height <= row || self.width <= col || self.x_off + col < 0 || self.y_off + row < 0
  || self.parent_x_off + col < 0 || self.parent_y_off + row < 0`) then
  `(*self.screen).write_cell((col as i32 + self.x_off) as u16, (row as i32 + self.y_off) as
  u16, cell)`. The `@intCast` casts are guarded non-negative by the preceding checks
  (faithful). Preserve `/// writes a cell to the location in the window`. **Mutating —
  `unsafe { (&mut *self.screen).write_cell(...) }`** (§5).
- **`readCell(self, col, row) -> Option<Cell<'a>>`** — same guard → `None`; else
  `unsafe { (&*self.screen).read_cell(...) }`. Read-only reborrow.
- **`clear(self)`** — `self.fill(Cell { default: true, ..Default::default() })`
  (Zig `.{ .default = true }`). Preserve `/// fills the window with the default cell`.
- **`gwidth(self, str: &[u8]) -> u16`** — `gwidth::gwidth(str, unsafe { (*self.screen)
  .width_method })`. **Read-only** deref (a `Copy` field read). Preserve `/// returns the
  width of the grapheme. This depends on the terminal capabilities`.
- **`fill(self, cell: Cell<'a>)`** — the contiguous/non-contiguous memset. Read
  `screen.width`/`screen.height`/`screen.buf.len()` and memset `screen.buf` ranges — a
  **mutating** reborrow held for the duration of the fill (`let s = unsafe { &mut
  *self.screen };` then all the range math + `s.buf[start..end].fill(cell)`). Port the
  early-return guard, the full-width contiguous fast path (`@memset` a single
  `start..end` slice, both `.min(buf.len())`), and the row-by-row `else` branch
  (`first_col`, `last_row`, per-row `start`/`end` with the triple `.min`). Preserve the
  `we have a full width window, therefore contiguous memory.` / `Non-contiguous. Iterate
  over rows an memset` comments (verbatim, including the typo `an`).
- **`hideCursor(self)`** — `unsafe { (*self.screen).cursor_vis = false }`. `/// hide the cursor`.
- **`showCursor(self, col, row)`** — guard (`x_off+col<0 || y_off+row<0 || row>=height ||
  col>=width`) then set `cursor_vis=true`, `cursor_row=(row as i32 + y_off) as u16`,
  `cursor_col=(col as i32 + x_off) as u16`. `/// show the cursor at the given coordinates,
  0 indexed`.
- **`setCursorShape(self, shape: cell::CursorShape)`** — `unsafe { (*self.screen)
  .cursor_shape = shape }`.

### 4.4 `print` / `printSegment` — see §6 (the load-bearing layout)

### 4.5 `scroll`, `hasMouse`, iterators

- **`scroll(self, n: u16)`** — `if n > self.height { return }`; per-row `@memcpy` shifting
  rows up by `n` over `screen.buf` (`copy_within`/`copy_from_slice` of `width`-length
  non-overlapping row slices), then `self.child(ChildOptions { y_off: (self.height - n) as
  i32, ..Default::default() }).clear()`. Preserve the two doc-comments. **Not exercised by
  a test but ported live** (mutating reborrow).
- **`hasMouse(win, mouse: Option<mouse::Mouse>) -> Option<mouse::Mouse>`** — pure; the
  `event.col >= win.x_off && event.col < win.x_off + win.width && event.row >= win.y_off &&
  event.row < win.y_off + win.height` box test. `event.col`/`row` are `u16`; `win.x_off`
  etc are `i32` — cast to compare (`event.col as i32 >= win.x_off`). Preserve the doc-comment.
- **`LineIterator`** — `struct { buf: &[u8], index: usize (0), has_break: bool (true) }`;
  `next(&mut) -> Option<&[u8]>` using `std.mem.indexOfAnyPos(u8, buf, index, "\r\n")` →
  a helper that finds the first `\r`/`\n` at/after `index`; on `None` (no break) set
  `has_break=false` **iff `start==0`**, `index=buf.len`, return `buf[start..]`; else set
  `index=end`, `consumeCR`, `consumeLF`, return `buf[start..end]`. Port `consumeCR`/
  `consumeLF` verbatim (+ the `// consumes a \n byte` / `// consumes a \r byte` comments)
  and the doc-comment `Lines are split by '\r', '\n', or '\r\n'`.
- **`WhitespaceTokenizer`** — `struct { buf: &[u8], index: usize (0) }`; `Token` enum
  `{ Whitespace(usize), Word(&[u8]) }` (Zig `whitespace: usize` = accumulated width with
  **Tab = 8**; `word: []const u8`). `next` scans a whitespace run (`' '`→+1, `'\t'`→+8,
  else break) or a word run (until `' '`/`'\t'`). Preserve `// the length of whitespace.
  Tab = 8` and `Returns tokens of text and whitespace`.

---

## 5. The `Window → Screen` back-reference — raw-pointer model (KEY DECISION)

**Chosen: `screen: *mut Screen<'a>`, `Window<'a>` is `Copy`; mutating methods take a
narrow transient `unsafe { &mut *self.screen }`, read methods `unsafe { &*self.screen }`.**

**Why NOT `&mut Screen`:** Zig `Window` is a **`Copy` value** (`self: Window` by value)
and `child()` returns a *new* Window sharing the SAME `*Screen` while the parent stays
alive — many live windows (root + every child, and inside `scroll` the parent AND its
temporary bottom-row child) all mutate one screen. `&mut Screen` is non-`Copy` and
forbids aliasing → it cannot model Copy-Window + child aliasing. **This is precisely the
"child-window aliasing forces a raw ptr" case the task calls out.** Interior mutability
(`RefCell`/`Cell` on Screen fields) would deform the faithful plain-field `Screen` shape
and is a heavier divergence than a documented raw pointer. So `*mut Screen<'a>` — a
byte-faithful match to Zig `*Screen`.

**Discipline (per `rust-mut-args-strongly-protected` + `shaper-hook-sb-clean-interleave`):**
- Every deref is a **fresh, short-lived, non-overlapping** reborrow created at the call
  site and dropped before the method returns; no long-lived `&mut` is held across another
  access to the same screen. `write_cell`/`show_cursor`/`set_cursor_shape`/`hide_cursor`
  take a one-statement `&mut`; `fill`/`scroll` take a single `&mut` for the whole body
  (no re-entrant screen access inside). `read_cell`/`gwidth`/`print`(commit=false) take
  `&*`.
- The pointer carries **write provenance**: at construction (tests + later `Vaxis`) it is
  `&mut screen as *mut Screen<'a>`, so a transient `&mut *p` is Stacked-Borrows-valid as
  long as `screen` outlives the windows and no overlapping `&mut screen` exists while a
  window is in use (the tests satisfy this — the `screen` local is untouched directly
  while `win` is alive).
- **Never dereferenced when absent:** the five "Window size" tests use a Zig `undefined`
  screen; their `initChild`/`readCell`/`writeCell` all bounds-guard-return **before** any
  deref (and `initChild` only copies the pointer). Port `undefined` → `std::ptr::null_mut()`
  (a `*mut Screen<'static>`); it is provably never read. PORT-NOTE this maps Zig's
  never-read `undefined`.

**Miri (R6):** the crate stays PURE (no FFI) → Miri is primary. The raw-ptr transient
reborrows are the only `unsafe`; they are SB-clean by the discipline above. The two print
tests deref read-only through a live stack `Screen`; the size tests never deref. If any
SB subtlety surfaces during the port, prove it on an isolated non-FFI Miri harness
(`.port/phase-2/mirihar-*`) rather than hand-reasoning — do NOT weaken the model.

**Lifetime threading:** `Window<'a> { screen: *mut Screen<'a>, … }` names `'a` via the
pointee. `write_cell`/`print` accept `Cell<'a>` / `Segment<'a>` so a written grapheme
(borrowed from segment text or a `'static` glyph) outlives the screen buffer — Zig's
caller-managed-lifetime contract, made checkable.

---

## 6. `print` / `printSegment` — the gwidth-aware grapheme-segmentation layout

This is the fidelity core (`test "print: grapheme"` + `test "print: word"` exercise the
c1b `gwidth`/`unicode` path). Signatures:

- `PrintOptions { row_offset: u16 (0), col_offset: u16 (0), wrap: Wrap (Wrap::Grapheme),
  commit: bool (true) }` where `Wrap = enum { Grapheme, Word, None }`. Preserve all option
  doc-comments (`vertical offset to start printing at`, the three wrap-variant comments,
  the `when true, print will write to the screen…` block).
- `PrintResult { col: u16, row: u16, overflow: bool }`.
- `print(self, segments: &[Segment<'a>], opts: PrintOptions) -> PrintResult`. Preserve
  `/// prints segments to the window. returns true if the text overflowed…`.
- `printSegment(self, segment: Segment<'a>, opts) -> PrintResult` = `self.print(&[segment],
  opts)` (+ the `/// This is just a shortcut for print(&.{segment}, opts)` comment).

**Grapheme arm** (`Wrap::Grapheme`): `col = col_offset`; label-`for` over segments with
`unicode::grapheme_iterator(segment.text)`; per grapheme:
`if col >= self.width { row += 1; col = 0 }`; `if row >= self.height { overflow = true;
break }`; `s = grapheme.bytes(segment.text)`; `if s == b"\n" { row = row.saturating_add(1);
col = 0; continue }` (the `+|=` saturating add — §"saturation"); `w = self.gwidth(s)`;
`if w == 0 { continue }`; `if opts.commit { self.write_cell(col, row, Cell { char:
Character { grapheme: s, width: w as u8 }, style: segment.style, link: segment.link,
wrapped: col + w >= self.width, ..Default::default() }) }`; `col += w`. After the segment
loop (Zig `} else false` — the `for…else`): `if col >= self.width { row += 1; col = 0 }`;
return `{ row, col, overflow }`.

**Word arm** (`Wrap::Word`): `col = col_offset`, `overflow = false`, `soft_wrapped =
false`; label-`outer` over segments; per segment a `LineIterator { buf: segment.text }`;
per `line`:
- the Zig **`defer`** block runs at each line-body exit (`if line_iter.has_break {
  soft_wrapped = false; row += 1; col = 0 }`). **CRITICAL (§risk):** Zig `defer` runs on
  the normal path AND while unwinding a `break :outer` that passes through the line-loop
  body. Rust has no `defer` — emulate by running the `has_break` bump on **every** exit of
  the line body, including the overflow-`break 'outer` path. Recommended structure: keep a
  small closure or an explicit inline copy of the bump immediately before the `break
  'outer`, and at the natural end of each line iteration. Do NOT drop the break-path bump —
  it is load-bearing for a line that both contains a linebreak and overflows (untested by
  the corpus but a real behavior). Preserve the comment `// We only set soft_wrapped to
  false if a segment actually contains a linebreak`.
- inner `WhitespaceTokenizer { buf: line }`; per token:
  - `Whitespace(len)`: `if soft_wrapped { continue }`; loop `0..len`:
    `if col >= self.width { col = 0; row += 1; break }`; `if opts.commit { write_cell(col,
    row, Cell { char: Character { grapheme: b" ", width: 1 }, style, link, ..default }) }`;
    `col += 1`.
  - `Word(word)`: `width = self.gwidth(word)`; `if width + col > self.width && width <
    self.width { row += 1; col = 0 }`; grapheme-iterate `word`: `soft_wrapped = false`;
    `if row >= self.height { overflow = true; break 'outer }`; `s = grapheme.bytes(word)`;
    `w = self.gwidth(s)`; `if opts.commit { write_cell(col,row, Cell{ char:{grapheme:s,
    width:w as u8}, style, link, ..default }) }`; `col += w`; `if col >= self.width { row
    += 1; col = 0; soft_wrapped = true }`.
- return `{ row, col, overflow }` (+ the `// remove last row counter` comment).

**None arm** (`Wrap::None`): `col = col_offset`; label-`for` over segments, grapheme-iterate
`segment.text`: `if col >= self.width { break overflow=true }`; `s = grapheme.bytes(...)`;
`if s == b"\n" { break overflow=true }`; `w = self.gwidth(s)`; `if w == 0 { continue }`;
`if opts.commit { write_cell(...) }`; `col = col.saturating_add(w)` (the `+|=`). `} else
false`; return `{ row, col, overflow }`.

**Dead trailing `return false` (fidelity):** Zig closes `print` with `return false;` after
an exhaustive `switch` whose every prong returns — unreachable dead code that Zig tolerates
but Rust rejects (wrong type + unreachable). **OMIT it** in Rust (the `match` is exhaustive
and each arm returns `PrintResult`). PORT-NOTE the omission.

**Saturation (`+|=` vs `+=`):** map Zig `row +|= 1` / `col +|= w` (grapheme-newline, none-arm)
to `saturating_add`; map plain `+= 1` / `+= w` (all other sites) to plain Rust `+=`
(overflow-panic-faithful — matches Zig checked `+`). Get each site right; the two are not
interchangeable.

**Fidelity check to run:** spot-check a wide-char / emoji print (e.g. a width-2 CJK or a
ZWJ emoji segment) so the `gwidth` advance + `wrapped: col + w >= width` + wrap boundaries
match the c1b widths — the two ported tests use `Method::Unicode` and pin col/row/overflow
for ASCII + newline; add no NEW asserting tests (test count is a contract) but verify the
wide path by inspection during the port.

---

## 7. R5 seams (none new) + fidelity quirks to preserve

**No new R5 seams.** Every `Screen`/`InternalScreen`/`Window` method ports LIVE (all deps
present). The crate's only seams stay in `image.rs` — `Image::draw`/`cellSize`
(`unimplemented!()` + verbatim Zig body, from 1a). **C2 does not touch `image.rs`**: even
though `Window`/`Screen` now exist (so `draw` *could* be un-seamed), that is a later
Image-focused chunk — out of scope, and the task lists "Image draw" as an R5 seam. Leave
`image.rs` unchanged.

**Fidelity quirks (port EXACTLY, do not "fix" — each is a real Zig behavior):**
1. **InternalScreen `<` bounds** (`if self.width < col`) vs Screen's `>=` — off-by-one
   asymmetry between the two buffers; preserve both.
2. **`InternalCell.default = true`** default (vs `Cell.default = false`) — drives `eql`'s
   fast-path.
3. **Dead `return false`** at the end of `print` — omit (§6).
4. **`i17 → i32`** offset widening (Rust has no `i17`).
5. **Allocator + error-union removal** on `Screen::init`/`InternalScreen::init` (infallible
   `Vec`); **arena → per-cell `Vec<u8>`**; **`log.warn`/`catch` blocks dropped**
   (unreachable in Rust).
6. **`readCell` u16-vs-usize index asymmetry** in InternalScreen (§3) — value-identical,
   note it.
7. **`clearRetainingCapacity` → `Vec::clear`** (capacity retained — exact).
8. **`undefined` → `null_mut()` (screen) / `Cell::default()` (cell)** in the size tests —
   never read (§5, §8).

---

## 8. The 9 ported tests

Port each Zig `test{}` to a `#[test]` (all in the respective `#[cfg(test)] mod tests`),
assertion-for-assertion:

1. **`Screen "refAllDecls"`** → `#[test] fn ref_all_decls()` — a smoke test that
   constructs a `Screen` (e.g. `let _ = Screen::default();` / touches `init`) so the
   module's public decls are reified. (Rust compiles all items regardless; this keeps the
   test COUNT contract, §10.)
2–5. **`Window size …`** — build `parent = Window { x_off:0, y_off:0, parent_x_off:0,
   parent_y_off:0, width:20, height:20, screen: std::ptr::null_mut() }`; call
   `parent.init_child(...)` with the same args; `assert_eq!` on `ch.width`/`ch.height`
   (tests 2–3), `ch.x_off`/`ch.y_off` (test 5), or `ch.width`/`ch.height` (test 4). No
   deref of `screen`. (`init_child` is private — the tests live in the same module so they
   can call it.)
6. **`Window offsets`** — `parent` (null screen), `ch = parent.init_child(10,10,Some(21),
   Some(21))`, `ch2 = ch.init_child(-4,-4,None,None)`; `assert!(ch2.read_cell(0,0).is_none())`
   (the `parent_x_off + col < 0` guard returns `None` before deref); `ch2.write_cell(0,0,
   Cell::default())` — bounds-guard no-op (Zig `undefined` cell → `Cell::default()`, never
   read). Preserve the two comments (`Reading ch2 at row 0 should be null`, `Should not
   panic us`).
7. **`print: grapheme`** — `let mut screen = Screen { width_method: Method::Unicode,
   ..Default::default() };` `let win = Window { …width:4, height:2, screen: &mut screen as
   *mut _ };` `let opts = PrintOptions { commit: false, wrap: Wrap::Grapheme, ..default };`
   then the 5 sub-blocks (`"a"` → col 1/row 0; `"abcd"` → 0/1; `"abcde"` → 1/1;
   `"abcdefgh"` → 0/2 overflow false; `"abcdefghi"` → 0/2 overflow **true**), each
   `assert_eq!` on `result.col`/`.row`/`.overflow`. `Segment { text: b"a", ..default }` —
   `Segment` needs a `Default`? Zig `.{ .text = "a" }` relies on `style`/`link` defaults;
   `Segment` currently has no `Default` (its `text` has none). Build segments explicitly:
   `Segment { text: b"a", style: Style::default(), link: Hyperlink::default() }` (a tiny
   ctor helper in the test module keeps it terse).
8. **`print: word`** — same screen/win, `wrap: Wrap::Word`; port ALL ~21 sub-blocks
   verbatim (`"a"`→1/0, `" "`→1/0, `" a"`→2/0, `"a b"`→3/0, `"a b c"`→1/1, `"hello"`→1/1,
   `"hi tim"`→3/1, `"hello tim"`→0/2 overflow, `"hello ti"`→0/2, the multi-segment `"h"`+
   `"e"`→2/0, the 5×1-char →1/1, `"he\n"`→0/1, `"he\n\n"`→0/2, `"not now"`→3/1, `"note
   now"`→3/1, `"note"`+`" now"`→3/1, `"note "`+`"now"`→3/1). These pin the word-wrap +
   newline + soft-wrap + multi-segment behavior including the `defer` semantics (§6).
9. **`Window "refAllDecls"`** → `#[test] fn ref_all_decls()` (window module) — smoke
   constructing a `Window` / touching decls.

**Total = 9** new `#[test]`s (7 behavioral + 2 refAllDecls smokes).

---

## 9. Deps / acyclic (R4)

No new crate deps — all consumed modules (`cell`, `gwidth`, `unicode`, `mouse`, `image`,
`Winsize`) are intra-crate. `rustty-vaxis` stays a leaf (its only edge is the existing
`rustty-vaxis → rustty-unicode`? — **no**: C1b vendored `uucode` INTO `rustty-vaxis`,
so the crate is dep-free/leaf). `check_acyclic.sh` trivially green. `Cargo.toml` unchanged.

---

## 10. Rules / gates

- **R1** — every line/comment/`test{}` ported assertion-for-assertion, incl. the
  InternalScreen `<`-bounds quirk, `InternalCell.default=true`, the dead `return false`
  omission, the `defer`-on-`break :outer` word-wrap semantics, the `-|`/`+|=` saturations,
  all section/inline comments (verbatim, incl. the `an` typo), the box-glyph consts.
- **Byte-slices** — `Character.grapheme`, `Segment.text`, `Hyperlink.uri/params`,
  `InternalCell.char/uri/uri_id`, the border glyphs → `&[u8]`/`Vec<u8>`, **never lossy
  `String`** (grapheme/UTF-8 bytes).
- **R4** — leaf crate, acyclic, no new deps.
- **R5** — no new seams; `image.rs` untouched (its `draw`/`cellSize` stay
  `unimplemented!()` + verbatim body).
- **R6** — PURE ⇒ **Miri primary**. The only `unsafe` is the `*mut Screen` reborrow
  discipline (§5); prove any SB subtlety on an isolated `mirihar-*` harness, don't
  hand-wave.
- **`check_test_counts.py` MAP update (REQUIRED):** the `"vaxis"` entry (line ~195) lists
  only `Cell/Key/Mouse/GraphemeCache/Image` on the zig side. **Add `Screen.zig`,
  `InternalScreen.zig`, `Window.zig`** (and, to tighten the now-loose margin, the already-
  landed `gwidth.zig`/`unicode.zig`) so the zig contract reflects the true count
  (Cell 1 + Key 5 + Screen 1 + Window 8 [+ gwidth 11] = 15 [or 26]); the rust glob
  `{RSVAXIS}/*.rs` already picks up the 9 new tests, keeping `rust >= zig`. Update the
  stale `zig=6; rust=6` banner comment. NOTE the gate is a coarse aggregate backstop (the
  gwidth margin means a single dropped Window test may not flip it red) — the real
  contract is the fidelity-verify pass + the explicit per-test port in §8.
- **`check_comment_coverage.py --crate rustty-vaxis`** — preserve every distinctive
  (≥5-token) comment: the offset/clip doc-block, the print/option doc-comments, the
  `Windows do not retain a reference to their parent` note, the custom-glyph index block,
  the `fastpath when both cells are default`, the `full width window…contiguous memory`
  pair, the LineIterator/WhitespaceTokenizer docs.
- **Gates:** `cargo build --workspace` / `cargo test -p rustty-vaxis` / `cargo clippy -p
  rustty-vaxis --all-targets -- -D warnings` / `cargo fmt --check` /
  `bash .port/tools/gates/check_acyclic.sh` / `python3 .port/tools/gates/check_test_counts.py`
  / `python3 .port/tools/gates/check_comment_coverage.py --crate rustty-vaxis` /
  `cargo +nightly miri test -p rustty-vaxis`.

---

## 11. Bottom line

Chunk 2 is **READY** and dep-free. Port `Screen` → `screen.rs` (`Vec<Cell<'a>>` owned
buffer, manual `Default`, allocator/error dropped), `InternalScreen` → `internal_screen.rs`
(arena → per-cell owned `Vec<u8>`, `<`-bounds + `default=true` quirks preserved, `eql`
faithful, `log`/`catch` dropped), and `Window` → `window.rs` (**`screen: *mut Screen<'a>`,
`Copy` window, narrow transient reborrow discipline §5** — forced by Copy-Window +
child-window aliasing; `i17 → i32`; the full `initChild`/`child`+border, cell/cursor/fill,
`scroll`/`hasMouse`, and the `print` grapheme/word/none layout §6 with the `defer`-on-break
subtlety, `-|`/`+|=` saturations, and the omitted dead `return false`). Port all **9
`test{}`** (7 behavioral + 2 `refAllDecls` smokes), update the `check_test_counts` MAP to
add the three files, and keep Miri primary over the raw-ptr seam. No new R5 seams;
`image.rs` stays as-is. Nothing blocked.
