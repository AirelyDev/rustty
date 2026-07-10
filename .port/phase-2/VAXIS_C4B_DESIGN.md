# VAXIS Chunk 4b — `widgets/TextInput.zig` → `widgets/text_input.rs`

Full-fidelity port of the vaxis 0.5.1 **TextInput** widget (the search box used by
`list_themes`). Source (read-only, 460 LOC, 3 `test{}`):
`…/vaxis-0.5.1-…/src/widgets/TextInput.zig`.
Target: `crates/rustty-vaxis/src/widgets/text_input.rs` + a new `widgets` module.

Disposition: **ready**. No blocker. The one non-mechanical decision is the `draw`
lifetime/aliasing shape (§6), resolved below with a record-then-replay pass.

---

## 1. What TextInput is

A single-line text-input widget built over a **gap buffer** of raw UTF-8 bytes. The
public surface:

- state: `buf: Buffer` (the gap buffer) + 4 `u16` draw/scroll counters
  (`draw_offset`, `prev_cursor_col`, `prev_cursor_idx`, `scroll_offset=4`).
- `init`/`deinit` (allocator-taking in Zig).
- `update(Event)` — the key-dispatch table (insert / delete / motion / word-motion).
- `insertSliceAtCursor`, `sliceToCursor`, `toOwnedSlice`, `clearAndFree`,
  `clearRetainingCapacity`, `reset`, `byteOffsetToCursor`.
- cursor motion: `cursorLeft`, `cursorRight`, `moveBackwardWordwise`,
  `moveForwardWordwise`.
- delete: `deleteBeforeCursor`, `deleteAfterCursor`, `deleteToEnd`, `deleteToStart`,
  `deleteWordBefore`, `deleteWordAfter`.
- rendering: `draw`, `drawWithStyle` (gwidth-aware, horizontal-scroll, ellipsis), plus
  helpers `widthToCursor`, `graphemesBeforeCursor`.
- nested `pub const Buffer = struct {…}` — the gap buffer itself (11 methods).

3 `test{}`: `"assertion"` (insert a 6× ZWJ-emoji smoke), `"sliceToCursor"`,
`"TextInput.zig: Buffer"` (the gap-buffer invariants).

It is **self-contained over `Window`** — it never touches `Vaxis`, `Loop`, or the tty.
So there is **no R5 seam**; every method ports LIVE. It is a **pure leaf** → R6 Miri is
the primary gate.

---

## 2. Confirmed intra-crate APIs it consumes (all landed)

| Zig | Rust (this crate) | Notes |
|---|---|---|
| `Key.matches(cp, .{…})` | `key::Key::matches(&self, cp: u32, mods: Modifiers) -> bool` | mods via `Modifiers{ ctrl:true, ..Default::default() }` |
| `Key.text` | `key::Key.text: Option<&[u8]>` | grapheme bytes, never `String` |
| `Key.codepoint` | `key::Key.codepoint: u32` | |
| `Key.backspace/delete/left/right/home/end` | `key::{backspace,delete,left,right,home,end}: u32` | free `const`s, lower-snake preserved |
| `unicode.graphemeIterator(s)` → `.next()` → `.len` / `.bytes(s)` | `unicode::grapheme_iterator(s)` → `GraphemeIterator::next() -> Option<Grapheme>`; `Grapheme.len`, `Grapheme.bytes(s) -> &[u8]` | Old-API wrapper, `.next()` is inherent (not `Iterator` trait) |
| `win.gwidth(g)` | `window::Window::gwidth(self, &[u8]) -> u16` | `Window` is `Copy`, methods take `self` by value |
| `win.width` | `window::Window.width: u16` | field |
| `win.writeCell(col,row,.{…})` | `window::Window::write_cell(self, u16, u16, Cell<'a>)` | screen **borrows** the grapheme `&'a [u8]` (does not copy) → drives §6 |
| `win.showCursor(col,row)` | `window::Window::show_cursor(self, u16, u16)` | |
| `Cell.Character` | `cell::Character<'a>{ grapheme: &'a [u8], width: u8 }` | `Default = { b" ", 1 }` |
| `Cell.Style` | `cell::Style` (`Default`) | passed opaque through `drawWithStyle` |
| `.{ .char = …, .style = … }` | `Cell{ char, style, ..Default::default() }` | |

`std.mem` helpers used by the word-motion code (`trimRight`, `lastIndexOfScalar`,
`indexOfScalarPos`, `copyForwards`) map to slice iterator methods / a manual
forward-copy (§5).

---

## 3. Module wiring (new `widgets` tier)

Mirror the Zig `src/widgets.zig` aggregator (`pub const TextInput = @import("widgets/TextInput.zig");`).

- **New dir** `crates/rustty-vaxis/src/widgets/` with `text_input.rs`.
- **New file** `crates/rustty-vaxis/src/widgets.rs` (the aggregator, doc-commented
  `//! Specialized TUI Widgets` per widgets.zig):
  ```rust
  pub mod text_input;
  pub use text_input::TextInput;   // so `vaxis::widgets::TextInput` names the struct,
                                   // matching Zig `widgets.TextInput`
  ```
- **`lib.rs`**: add `pub mod widgets;` (+ a one-line CHUNK 4b doc bullet).

**R4 acyclic**: `widgets` sits ABOVE `window` and depends only DOWNWARD
(`cell`, `key`, `window`, `gwidth`, `unicode`). No lower module gains a `widgets`
edge. `check_acyclic.sh` stays green.

---

## 4. The gap `Buffer` — representation (pin)

Zig `Buffer` owns a raw `buffer: []u8` split into `firstHalf = buffer[0..cursor]`,
a gap of `gap_size`, and `secondHalf = buffer[cursor+gap_size..]`.

**Rust representation** — faithful, allocator-free (crate convention, cf. `Screen`):

```rust
pub struct Buffer {
    pub buffer: Vec<u8>,   // Zig `buffer: []u8` — the WHOLE allocation (halves + gap)
    pub cursor: usize,     // bytes before the gap
    pub gap_size: usize,   // gap length in bytes
    // Zig `allocator` field: DROPPED (Rust global alloc; Vec owns/frees itself)
}
```

Key points:

- **`Vec<u8>` as the whole slab, `buffer.len()` = the Zig allocation length.** The gap
  region is never read (Zig invariant "the gap is never within a grapheme"), so `grow`
  builds a fresh `vec![0u8; new_size]` — the zero-fill of the gap is a **benign,
  Miri-safe divergence** from Zig's `undefined` gap bytes, identical to the
  `GraphemeCache` PORT-NOTE.
- `firstHalf`/`secondHalf` → `&self.buffer[..self.cursor]` / `&self.buffer[self.cursor + self.gap_size..]` returning `&[u8]`.
- `init()` → `Buffer{ buffer: Vec::new(), cursor: 0, gap_size: 0 }` (Zig `buffer = &.{}`
  = empty slice = empty Vec). `deinit`/`clearAndFree` → set `buffer = Vec::new()`
  (Vec `Drop` frees); the shim is kept for API parity like `Screen::deinit`.
- **Allocator error dropped**: Zig `grow`/`insertSliceAtCursor`/`toOwnedSlice` return
  `std.mem.Allocator.Error!void`. Rust `Vec` alloc is infallible (aborts on OOM) →
  these become non-`Result` (`-> ()` / `-> Vec<u8>`), matching the crate model
  (`Screen`, `InternalScreen`). The `try` at every call site drops.
- `grow(n)`: `new_size = buffer.len() + n + 512`; new `vec![0u8; new_size]`; copy
  first half to `[..cursor]`, second half to the tail `[new_size - sh.len()..]`;
  `gap_size = new_size - sh.len() - cursor`. (Copy from the OLD `buffer`, then replace
  it — take a snapshot of `secondHalf` before overwriting.)
- `toOwnedSlice()` → allocate `Vec<u8>` of `first.len()+second.len()`, copy both halves,
  `clearAndFree`, return the `Vec` (Zig `![]const u8`, caller-owned).
- `realLength` ported (dead but R1 line-for-line).

### 4.1 `moveGapLeft`/`moveGapRight` — `copyForwards` fidelity (subtle)

Zig uses `std.mem.copyForwards(u8, dst, src)` — a **forward** element copy that, for
overlapping ranges with `dst > src`, deliberately does NOT behave like `memmove`.
`slice::copy_within` IS `memmove` and would diverge on that overlap. To be byte-identical
to Zig, port a **manual forward-copy over one raw pointer** (both ranges are subslices of
the same `Vec`, so borrow-splitting is impossible; a single `*mut u8` walk is the honest
mirror and is clean under Miri — one pointer, no aliasing):

```rust
// faithful std.mem.copyForwards: for i in 0..n { dst[i] = src[i]; } (forward order)
unsafe {
    let p = self.buffer.as_mut_ptr();
    for i in 0..n { *p.add(dst_start + i) = *p.add(src_start + i); }
}
```

- `moveGapLeft(n)`: `new_idx = cursor -| n` (saturating); `dst=[new_idx+gap_size..]`,
  `src=[new_idx..cursor]` (len `cursor-new_idx`); `cursor = new_idx`.
- `moveGapRight(n)`: `new_idx = cursor + n`; `dst=[cursor..]`,
  `src=[cursor+gap_size .. new_idx+gap_size]` (len `new_idx-cursor`); `cursor = new_idx`.
- Saturating `-|`/`-|=` → `usize::saturating_sub`. `@min` → `.min()`.
- `insertSliceAtCursor(slice)`: early-return on empty; `if gap_size <= slice.len() { grow(slice.len()) }`; `buffer[cursor..cursor+len].copy_from_slice(slice)`; bump `cursor`, shrink `gap_size`.
- `growGapLeft(n)`: `gap_size += n; cursor -|= n`.
- `growGapRight(n)`: `gap_size = (gap_size + n).min(buffer.len() - cursor)`.
- `clearRetainingCapacity`: `cursor = 0; gap_size = buffer.len()`.

---

## 5. TextInput methods (mechanical mappings)

- **`Event`**: a widget-local union `union(enum){ key_press: Key }` →
  `pub enum Event<'a> { KeyPress(Key<'a>) }`. `'a` because `Key<'a>` borrows `text`.
  NOT the crate-level `event::Event`.
- **`update(&mut self, event: Event<'_>) `** (non-`Result`; `insertSliceAtCursor` no
  longer fallible). The exact `matches` chain, in order — each `.{ .ctrl = true }` →
  `Modifiers{ ctrl: true, ..Default::default() }`, `.{}` → `Modifiers::default()`,
  `.{ .alt = true }` → `Modifiers{ alt: true, ..}`. The `if key.text |text|` tail →
  `if let Some(text) = key.text { self.insert_slice_at_cursor(text); }`.
- **`insertSliceAtCursor(&mut self, data: &[u8])`**: `grapheme_iterator(data)`,
  `while let Some(g) = iter.next()` → `self.buf.insert_slice_at_cursor(g.bytes(data))`.
- **`sliceToCursor(&self, buf: &mut [u8]) -> &[u8]`**: `assert!(buf.len() >= self.buf.cursor)`;
  `buf[..cursor].copy_from_slice(self.buf.first_half())`; return `&buf[..cursor]`.
  (Zig sig is `*TextInput` but read-only → `&self`; PORT-NOTE the demotion.)
- **`widthToCursor(&self, win) -> u16`**: iterate first-half graphemes, skip `i < draw_offset`,
  `width += win.gwidth(g)`.
- **`cursorLeft`/`deleteBeforeCursor`**: walk first-half graphemes keeping the LAST
  `grapheme.len` (Zig `len = grapheme.len` each iter), then `moveGapLeft(len)` /
  `growGapLeft(len)`.
- **`cursorRight`/`deleteAfterCursor`**: `let Some(g) = second-half iter.next() else { return }`;
  `moveGapRight(g.len)` / `growGapRight(g.len)`.
- **home** (`ctrl-a`/Home): `moveGapLeft(first_half().len())`. **end** (`ctrl-e`/End):
  `moveGapRight(second_half().len())`.
- **`graphemesBeforeCursor(&self) -> u16`**: count first-half graphemes.
- **`deleteToEnd`**: `growGapRight(second_half().len())`. **`deleteToStart`**:
  `growGapLeft(self.buf.cursor)`.
- **`moveBackwardWordwise`**: trim trailing spaces from `first_half()`
  (`&fh[..fh.len() - fh.iter().rev().take_while(|&&b| b==b' ').count()]`), then
  `idx = trimmed.iter().rposition(|&b| b==b' ').map_or(0, |l| l+1)`
  (Zig `lastIndexOfScalar … |last| last+1 else 0`); `moveGapLeft(self.buf.cursor - idx)`.
- **`moveForwardWordwise` / `deleteWordAfter`**: skip leading spaces in `second_half()`
  (`i`), then `idx = second_half[i..].iter().position(|&b| b==b' ').map_or(len, |p| i+p)`
  (Zig `indexOfScalarPos(…, i, ' ') orelse len`); `moveGapRight(idx)` /
  `growGapRight(idx)`.
- **`deleteWordBefore`**: `let pre = self.buf.cursor; self.move_backward_wordwise(); self.buf.grow_gap_right(pre - self.buf.cursor);`.
- **`byteOffsetToCursor(&self) -> usize`**: `self.buf.cursor` (Zig by-value `self` → `&self`).
- **`clearAndFree`/`clearRetainingCapacity`/`toOwnedSlice`/`reset`**: delegate + `reset()`
  as in Zig. `toOwnedSlice -> Vec<u8>` with the `reset()` on `defer` → do it last before return.
- **`ellipsis`** → `const ELLIPSIS: Character<'static> = Character { grapheme: "…".as_bytes(), width: 1 };` (`str::as_bytes` is `const`).

---

## 6. `draw` / `drawWithStyle` — the one real design decision (aliasing + lifetime)

`win.write_cell(col,0, Cell{ char: Character{ grapheme: g, width }, style })` stores the
grapheme slice `g` **by reference** — `Screen<'a>` keeps `Cell<'a>` with
`grapheme: &'a [u8]` (confirmed in `screen.rs`; it borrows, never copies). In `draw`, `g`
points **into `self.buf.buffer`**. `Window<'a>` is **invariant** in `'a`
(`*mut Screen<'a>` → `Vec<Cell<'a>>` → `&'a [u8]`), so `'a` is fixed to the caller's
screen lifetime and cannot be shortened to the call.

Therefore the honest signature is:

```rust
pub fn draw(&'a mut self, win: Window<'a>)                 // where Self = TextInput
pub fn draw_with_style(&'a mut self, win: Window<'a>, style: Style)
```

i.e. the widget is borrowed for the **screen's** lifetime — exactly Zig's implicit
contract ("the caller must keep the text alive"; the screen holds pointers into
`self.buf` until those cells are overwritten).

**The conflict**: Zig's single render loop *interleaves* (a) `writeCell(g)` — which lends
`&'a self.buf` into the screen for `'a` — with (b) `self.prev_cursor_col = col` — a
`&mut self` write. Once a `&'a`-lived shared borrow of `self` escapes into the screen, the
`&mut self` is dead for the rest of `'a`; the two cannot interleave in Rust.

**Chosen resolution — record-then-replay (single decision loop, no logic duplication):**

1. **Pass A — mutate self, read `buf` only** (`&mut self`, no `'a` escape). Run the
   *actual* Zig decision loop once: compute `cursor_idx = graphemes_before_cursor()`,
   the `if cursor_idx < draw_offset { draw_offset = cursor_idx }`, the `win.width==0`
   early return, the `while` scroll-adjust of `draw_offset`, then set
   `prev_cursor_idx = cursor_idx` and `prev_cursor_col = 0`. Then walk both halves with
   the **exact** break conditions (`col + w >= win.width` first half; `col + w > win.width`
   second half; the `< draw_offset` skips; `if i == cursor_idx { prev_cursor_col = col }`),
   **recording** each draw op into a scratch `Vec` instead of calling `write_cell`:
   ```rust
   enum Op { Cell { col: u16, start: usize, len: usize, width: u8 }, Ellipsis { col: u16 } }
   ```
   `start/len` are the **absolute** range into `self.buf.buffer` (first-half grapheme →
   `[o..o+l]`; second-half grapheme → `[cursor+gap_size+gs .. +l]`), so replay needs no
   half bookkeeping. Also record the trailing `if draw_offset > 0 { Ellipsis{col:0} }`.
   All `self` mutation (`draw_offset`, `prev_cursor_idx`, `prev_cursor_col`) completes here.

2. **Pass B — replay, read self shared for `'a`.** Downgrade to `&'a self`; for each `Op`
   emit `win.write_cell(...)` (`Cell` char = `ELLIPSIS` or `Character{ grapheme: &self.buf.buffer[start..start+len], width }`), then `win.show_cursor(self.prev_cursor_col, 0)`.

This is behaviorally identical to Zig (same cells, same order, same
`prev_cursor_*`/`draw_offset` final values, same `showCursor`) and keeps a **single**
decision loop (the break/skip logic is written once, in Pass A). Cost: one small
`Vec<Op>` alloc per `draw` — a documented, benign divergence (Zig streams; the tests do
not exercise `draw`, so this is R1-line-faithful, not behavior-observable). PORT-NOTE this
prominently.

> Alternative considered — a two-pass *re-simulation* (Pass A computes `prev_cursor_col`
> by re-walking, Pass B re-walks to `write_cell`): rejected because it **duplicates** the
> break/skip logic in two places, inviting drift. Record-replay writes it once.

`draw(&'a mut self, …)` delegating to `draw_with_style(self, win, Style::default())`
(Zig `.{}`). Note the `@intCast(w)` grapheme width → `w as u8` (faithful truncation).

---

## 7. The 3 `test{}` (fidelity contract)

Ported assertion-for-assertion into `#[cfg(test)] mod tests`:

1. **`assertion`** — `astronaut = "👩‍🚀"`; build `Key{ codepoint: utf8Decode(astronaut[0..4]),
   text: Some(astronaut) , shifted_codepoint: None, base_layout_codepoint: None,
   mods: default }`. `utf8Decode(astronaut[0..4])` → decode the first 4 bytes (U+1F469)
   via `std::str::from_utf8(&astronaut[..4]).unwrap().chars().next().unwrap() as u32`.
   `for _ in 0..6 { input.update(Event::KeyPress(k)); }`. Smoke test (Zig has no explicit
   assert — it exercises `insertSliceAtCursor` + `grow`; success = no panic). Keep it a
   smoke test (no invented asserts).
2. **`sliceToCursor`** — `insert_slice_at_cursor(b"hello, world")`; 3× `cursor_left`;
   `assert_eq!(input.slice_to_cursor(&mut buf), b"hello, wo")`; `cursor_right`;
   `assert_eq!(…, b"hello, wor")`. `buf: [u8; 32]`. Use `&[u8]` equality
   (`expectEqualStrings` → byte-slice `assert_eq!`, never a lossy `String`).
3. **`TextInput.zig: Buffer`** — the gap-buffer invariants: `insert "abc"` →
   first=`b"abc"` second=`b""`; `move_gap_left(1)` → `b"ab"`/`b"c"`; `insert " "` →
   `b"ab "`/`b"c"`; `grow_gap_left(1)` → `b"ab"`/`b"c"`, `cursor==2`; `grow_gap_right(1)`
   → `b"ab"`/`b""`, `cursor==2`. All via `assert_eq!` on `&[u8]` / `usize`.

`byteOffsetToCursor`/`realLength`/`clearRetainingCapacity`/`toOwnedSlice`/`reset`/
`widthToCursor`/`draw*` are not test-covered by Zig; keep the same coverage (no
gold-plating) — R1 line fidelity + Miri is their gate.

---

## 8. Gates & the gate-config updates this chunk REQUIRES

Standard leaf gate set: `cargo build -p rustty-vaxis`, `cargo test -p rustty-vaxis`,
`cargo clippy -p rustty-vaxis -- -D warnings`, `cargo fmt --check`,
`check_acyclic.sh`, `check_test_counts.py`, `check_comment_coverage.py`, **Miri**
(pure leaf → primary: `cargo +nightly miri test -p rustty-vaxis text_input` — the
raw-ptr `copyForwards`, the gap-buffer moves, and `grow` are the Miri-relevant paths).

**Two gate-config edits are needed** (flag for the implementation step):

1. **`check_test_counts.py`** (`~line 200`): the `"vaxis"` mapping (a) must add
   `f"{GHVAXIS}/widgets/TextInput.zig"` to the Zig source list (raising the expected Zig
   count 68 → **71**), and (b) the Rust glob `[f"{RSVAXIS}/*.rs"]` is **non-recursive** and
   will MISS `widgets/text_input.rs` — change to `[f"{RSVAXIS}/*.rs", f"{RSVAXIS}/widgets/*.rs"]`
   (or `{RSVAXIS}/**/*.rs`). Without both, the gate under-counts and goes red.
2. **`check_comment_coverage.py`**: invoked per-file via `--port-glob` (recursive-capable),
   so it just needs to be run against `crates/rustty-vaxis/src/widgets/text_input.rs`
   (+ its Zig source) — no code change, only the invocation target.

`clippy`: pre-empt `should_implement_trait` on `Buffer`/iterator-less inherent methods is
n/a here; `Event` may trip `enum_variant_names`/single-variant — allow with a PORT-NOTE if
so (faithful to the Zig single-field union). `slice_to_cursor`/`byte_offset_to_cursor`
taking `&self` (Zig `*TextInput`/by-value) — PORT-NOTE the demotion.

---

## 9. Fidelity ledger (R1/R4/R5/R6)

- **R1**: every method, the nested `Buffer`, all doc/line comments, and all 3 `test{}`
  ported line-for-line. Grapheme bytes stay `&[u8]`/`Vec<u8>` — never a lossy `String`.
  Documented benign divergences: dropped allocator+`Allocator.Error` (crate model),
  zero-filled gap (vs `undefined`), the `draw` record-replay `Vec<Op>`,
  `&self` demotions. `@intCast` truncations preserved.
- **R4**: `widgets` is a new TOP tier; edges are strictly downward. Acyclic holds.
- **R5**: **none** — TextInput is self-contained over `Window`; no `Vaxis`/`Loop`/tty
  coupling, so nothing is deferred.
- **R6**: pure leaf → **Miri primary** over the gap-buffer / raw-ptr copy paths.

---

## 10. Deliverables

- `crates/rustty-vaxis/src/widgets/text_input.rs` — the widget + `Buffer` + 3 tests.
- `crates/rustty-vaxis/src/widgets.rs` — aggregator (`pub mod text_input; pub use …TextInput`).
- `crates/rustty-vaxis/src/lib.rs` — `pub mod widgets;` + CHUNK 4b doc bullet.
- `.port/tools/gates/check_test_counts.py` — vaxis mapping: +TextInput.zig, +widgets glob.

**Disposition: ready.**
