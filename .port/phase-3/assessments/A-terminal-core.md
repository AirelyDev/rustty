# Phase 3 — Area A re-vetting assessment: terminal core

Oracle range: `4749c4e93` (pin, 2026-06-20) → `origin/main` `6ad1fe7d8` (2026-07-29), 365 commits.
Rustty side: `main` @ `19d8a10`, assessed in worktree `agent-a288a8d58c8621005`.
Author: area-A agent. Date: 2026-07-29.

> **NOTE on missing Phase 3 artifacts.** `.port/phase-3/README.md`,
> `.port/phase-3/UPSTREAM_DIFF.patch` and `.port/phase-3/UPSTREAM_COMMITS.txt` **do not
> exist** on `main` at the time of this assessment (`.port/` contains only
> `phase-0/1/2`, `audits`, `tools`, `ORACLE_SYNC.md`, `PORT_RULES.md`,
> `cycle-analysis.md`). I therefore derived the range and commit list directly from the
> oracle repo at `/Users/gael/projects/ghostty` (working tree left untouched at the pin;
> all reads via `git show origin/main:…` / `git diff 4749c4e93..origin/main`). The
> range I used matches the numbers in the task brief (365 commits), so I believe it is
> the intended range — but a human should confirm and land the README/artifacts.

---

## 1. Summary (headline numbers)

Area-A file set actually touched in the range (the brief's list, plus the
Screen/Page-adjacent files no other area owns):

| upstream file | changed lines |
|---|---:|
| `src/terminal/PageList.zig` | 6518 |
| `src/terminal/Terminal.zig` | 4119 |
| `src/terminal/Screen.zig` | 2626 |
| `src/terminal/page.zig` | 470 |
| `src/terminal/mem.zig` | 222 (NEW) |
| `src/datastruct/segmented_list.zig` | 533 (NEW) |
| `src/terminal/Selection.zig` | 130 |
| `src/terminal/bitmap_allocator.zig` | 81 |
| `src/terminal/style.zig` | 34 |
| `src/terminal/Tabstops.zig` | 28 |
| `src/terminal/ScreenSet.zig` | 18 |
| `src/terminal/StringMap.zig` | 9 |
| `src/terminal/ref_counted_set.zig` | 9 |
| **total** | **~14,797** |

`src/terminal/size.zig` and `src/terminal/Pin.zig` are **not** in the range diff
(`Pin` lives inside `PageList.zig` upstream); nothing to assess there.

**79 distinct upstream commits** touch this file set. Verdicts:

| verdict | count |
|---|---:|
| **ABSENT** | **32** |
| **PARTIAL** | **1** |
| PORTED | 40 |
| N/A (mechanical / no Rust analogue / no area-A consumer / merge commit) | 6 |

By volume, roughly **60–65 % of the changed lines in this area are genuinely absent**
from rustty. The absent mass is concentrated in five clusters:

1. **Runtime scrollback limits / `Limits` type** (5 commits, ~1,900 changed lines) — a
   structural refactor of `PageList` limit state that also *replaces* the existing
   byte-limit pruning path. Widest blast radius of anything in this area.
2. **Resize error-safety + full-featured resize** (5 commits, ~1,850 lines) — new
   `Terminal.Resize`/`ResizeError`, `Screen.resize` rollback, tripwire fail points,
   alt-screen replacement fallback, cell pixel geometry, sync-output reset.
3. **The `printSlice` family** (4 commits, ~1,000 lines in `Terminal.zig` + 274 in
   `page.zig`) — batched codepoint runs, `printSliceFill`, and the `page.Mask`
   comptime masked-compare machinery that `render.zig` (area not mine) also consumes.
4. **BL-3 safety fixes, PageList half** (10 commits, ~700 lines) — **this is the
   biggest surprise: the `Screen.zig`/`Selection.zig` half of this series was ported
   and the `PageList.zig` half was not.** Several are *live panics in rustty today*
   (§3).
5. **Page-map probe bounds** (3 commits; `page.zig` side ~40 lines, `hash_map.zig`
   body owned by another area).

Ledger reliability, measured: I cross-referenced the 57 upstream hashes cited in
rustty-vt source comments against the 79 area-A commits. **33 were cited, 46 were
not — yet 7 of those 46 uncited commits are in fact fully ported** (`607160657`,
`fa8cae88b`, `a9f5b7eba`, `b6f34be44`, `0c299000f`, `cfce1cd56`, `f6fd4cb08`), and
one *cited* family (`c442eb4b3`) is incomplete. **Hash citations are not a usable
proxy for coverage in either direction.** Every row below was verified by reading
the upstream hunk and the corresponding rustty code.

### Coverage honesty

**Assessed exhaustively** (upstream hunk read + rustty code located and read):
all 79 commits' code hunks for `PageList.zig`, `Screen.zig`, `Terminal.zig`,
`page.zig`, `Selection.zig`, `Tabstops.zig`, `style.zig`, `mem.zig`; plus a
symbol-level `pub fn`/`fn` inventory diff of `PageList.zig`/`Screen.zig`/
`Terminal.zig`/`page.zig` between pin and main, cross-checked against rustty
(this is how I bounded "did I miss a whole function" — see §6).

**Sampled, not exhaustive** — stated explicitly so it is not mistaken for coverage:

- **`e8525c0fd` (Zig 0.16.0)** touches 2,852 lines across my four big files. I read
  the **complete** `page.zig` hunk set (all mechanical: `posix.PROT` bitfield form,
  `windows.exp.kernel32`, `content.codepoint` → `packed struct(u24){data,_pad}`) and
  **sampled** the `PageList`/`Screen`/`Terminal` hunks. Every hunk I read was a Zig
  language/stdlib migration with no behavioural delta. I did **not** prove the absence
  of a semantic change hidden in the unsampled ~2,000 lines. **Recommend a dedicated
  mechanical-sweep pass over `e8525c0fd` before closing area A.**
- **BL-1 compression series** (`95685afd2`, `181254d36`, `f8c217e55`, `e7969ed43`,
  `70e788f06`, `9156ada16`, `421fe8dab`) — verified at the level of *hooks, symbols
  and parity-tagged tests* (all 9 non-test `markActivity` call sites present and in
  the right functions; `compress`/`compress_incremental`/`compress_full`/
  `CompressionIterator`/`memory_stats` present). I did **not** re-diff the codec
  hunk-by-hunk: the codec is area B's and **a concurrent lane is editing
  `crates/rustty-vt/src/{compress,pagelist}/` right now**, so any line-level verdict
  I gave would be stale on arrival. Area B must own the line-level verdict.
- **`hash_map.zig` bodies** for `7e14347c1` / `fedd42e8d` / `65f953e8e` — explicitly
  another area's. I assessed only the `page.zig` / `Screen.zig` call sites.

---

## 2. Per-change verdict table

Legend for cost: **S** ≤ ~50 lines of Rust, **M** ~50–400, **L** > 400 or
cross-crate ripple.

### 2.1 Scrollback limits (runtime, byte + line)

| commit(s) | what changed | status | evidence | guarding test/gate | cost | risk |
|---|---|---|---|---|---|---|
| `5b2d3b7df` | `PageList.init` takes `Options{cols,rows,max_size:?usize,max_lines:?usize}`; new `explicit_max_lines`/`min_max_lines`, `minMaxLines`, `maxLines`, `enforceMaxLines`, `IntegrityError.MaxLinesExceeded`, `fixupViewport` clears `viewport_pin.garbage`, `enforceMaxLines` called from `clone`/`resize`/`resizeWithoutReflow`/`grow` | **ABSENT** | `crates/rustty-vt/src/pagelist/mod.rs:1379` `pub fn init(alloc, cols, rows, max_size: Option<usize>)` — 3 positional params, no `Options`. `grep -rn "max_lines\|maxLines" crates/rustty-vt/src` → **0 hits**. `IntegrityError` at `mod.rs` has no `MaxLinesExceeded` | none — needs one (port upstream's `MaxLinesExceeded` integrity check + the 6 new `PageList` tests) | **L** | `init` has **256 call sites in 8 rustty files** (`pagelist/tests.rs`, `screen.rs`, `search/pagelist.rs`, `pagelist/bench.rs`, `examples/*`, `rustty-inspector`). Mechanical but huge. |
| `86f81fb5b` | `Screen.Options.max_scrollback` → `max_scrollback_bytes` + new `max_scrollback_lines: ?usize`; same on `Terminal.Options`; plumbed through `ScreenSet`, `Selection`, `StringMap`, `Tabstops` tests | **ABSENT** | rustty still spells it `max_scrollback`: `crates/rustty-vt/src/screen.rs`, `terminal.rs`, `render.rs:1369`, `string_map.rs:342` … | none | **M** (mechanical) | **`max_scrollback` has 204 occurrences across 45 files in 9 crates** (`rustty-vt`, `rustty-vt-c`, `rustty-bench`, `rustty-core`, `rustty-renderer`, `rustty-font-shaper`, `rustty-termio`, `rustty-apprt-c`, `rustty-inspector`). Rename must land as one atomic commit or the workspace won't build. |
| `10bc43420` | `Screen`/`Terminal` treat byte limit as optional (`0` = disable scrollback, distinct from unlimited) | **ABSENT** | rustty `screen.rs` still derives `no_scrollback` from `max_scrollback == 0` with no "unlimited" encoding | none | **S** | semantic change to what `0` means; must land with `86f81fb5b`. |
| `f4c68d65e` | Extracts a `Limits{bytes:Limit,lines:Limit}` type with `Limit{explicit,min}`, `Limits.init/set/resize/max/exceeded/enforce`; **removes `PageList.maxSize`, `minMaxSize`, `enforceMaxLines`**; adds `PageList.setMaxBytes`/`setMaxLines` and `Terminal.setScrollbackMaxBytes`/`setScrollbackMaxLines`; **relocates the existing byte-limit pruning into `Limits.enforce`** | **ABSENT** | `grep -rn "setMaxBytes\|set_max_bytes\|set_max_lines\|struct Limits" crates/rustty-vt/src` → 0. rustty still has `pub fn max_size()` at `pagelist/mod.rs:2175` and `fn min_max_size()` at `mod.rs:1254` (both deleted upstream) | none | **L** | This is the one that **rewrites rustty's existing `grow()` byte-prune path**, not just adds to it. Must land *after* `5b2d3b7df` (it refactors that commit's output). |
| `739603b8a` | PR merge of the four above (+ `src/config` side, not mine) | **N/A** | merge commit, no unique diff (`git log -1 --format=%P` shows 2 parents) | — | — | config-side `scrollback-limit-lines` belongs to the config area. |

### 2.2 Resize (error-safety + features)

| commit(s) | what changed | status | evidence | guarding test/gate | cost | risk |
|---|---|---|---|---|---|---|
| `dde3d4d6b` | `Screen.resize` made failure-safe: `resize_tw` tripwire (`saved_cursor_pin`, `pages` fail points), saved-cursor pin tracked **before** any mutation, extra `styles.use`/`hyperlink_set.use` reference held across the resize so `errdefer` can restore the cursor without allocating, cursor node + serial saved to detect page survival, new private `clearPromptForRedraw` | **ABSENT** | `crates/rustty-vt/src/screen.rs:2396` `resize` → `resize_inner` is verbatim pre-fix (releases cursor style, `defer`-style restore, no extra refs, no rollback). `grep -n clear_prompt_for_redraw crates/rustty-vt/src/screen.rs` → **0 hits** | none — needs FailingAllocator/tripwire tests (rustty already has 19 tripwire sites in `screen.rs`, so the mechanism exists) | **L** | **Landmine:** the extra-reference trick plus `errdefer` restore must not hold a `&mut Page`/`&mut Screen` across `PageList::resize`. Use raw-pointer + narrow-access (see §5). |
| `a3c1caba5` | `Terminal.resize` failure-safe: rolls back tabstops, `width_px`/`height_px`, synchronized-output mode, screen dims; new `resize_tw` fail points (`tabstops`, `primary_screen`, `alternate_screen`, `alternate_screen_init`); `errdefer comptime unreachable` after commit point | **ABSENT** | `crates/rustty-vt/src/terminal.rs:4557` `pub fn resize(&mut self, alloc, cols, rows)` — no rollback, no fail points, no alt-screen recovery | the `91f0cf67d` tabstop hunk **is** ported here (`terminal.rs:4579`, guarded by `terminal_resize_preserves_tabstops_on_allocation_failure`); the rest is unguarded | **L** | — |
| `89b103dd5` | `Terminal.Resize{cols,rows,cell_size_px:?{width,height}}` + `ResizeError{OutOfMemory,InvalidValue}`; rejects 0-dim; sets pixel geometry even when grid unchanged; **resets `synchronized_output` on every valid resize**; alt-screen replace-or-fall-back-to-primary recovery. (Also adds `TerminalStream.resize` — `stream_terminal.zig`, not my area.) | **ABSENT** | same site, `terminal.rs:4557`. `grep -n "cell_size_px\|InvalidValue" crates/rustty-vt/src/terminal.rs` → 0 | none | **L** | Signature change ripples to `rustty-vt-c` (`c/terminal.zig` −49/+… upstream), `rustty-termio`, `rustty-core`. Coordinate with the `c/*` and `termio` areas. |
| `b988efcfe` | `alt.io` → `self.io()` inside the `a3c1caba5` alt-screen block (Zig-0.16 translation regression fix) | **N/A** | folds into the `a3c1caba5` port; rustty has no `Terminal.io()` accessor and doesn't need one | — | — | just don't reproduce the pre-fix form. |
| `ee9d5b352` | Page-capacity errors in `eraseRow`: new `IncreaseCapacity.forCloneError(err)` mapping, new `PageList.cloneRowGrowCapacity`, retry-on-capacity in `eraseRow`; new `Screen.clonePartialRowGrowCapacity`; `Terminal` call-site updates | **ABSENT** | `grep -n "for_clone_error\|clone_row_grow_capacity" crates/rustty-vt/src/pagelist/*.rs` → 0; `grep -n clone_partial_row_grow_capacity crates/rustty-vt/src/screen.rs` → 0. `erase_row` at `pagelist/erase.rs:24` still calls `clone_row_from` once with no retry | none | **L** | The Zig version `@panic`s on `increaseCapacity` failure mid-rotation (deliberately). Port the panic, don't invent a recovery. |
| `043326249` | Page-capacity errors in `cursorScrollAboveRotate`: `current_is_fresh` tracking + `assert(fresh_node == null or fresh_node == pages.last)`, inner retry loop with the same error→dimension mapping | **ABSENT** | `crates/rustty-vt/src/screen.rs` `cursor_scroll_above_rotate` has no retry loop / `current_is_fresh` | none | **M** | Should land *with* `ee9d5b352` and share the `forCloneError` helper. |
| `c594031d5` | New `Terminal.Cursor{is_default,default_style,default_blink}` + `Options.default_cursor_style`/`default_cursor_blink`; new `Terminal.setCursorStyle`, `setDefaultCursorStyle`, `setDefaultCursorBlink`; DECSCUSR logic **moved out of** `termio/stream_handler.zig` (−68) and `stream_terminal.zig` (−33) into `Terminal` | **ABSENT** | `grep -n "fn set_cursor_style\|default_cursor_style\|is_default" crates/rustty-vt/src/terminal.rs` → 0 hits | none | **M** | Cross-area: the *deletions* live in `stream_terminal.zig` (not mine) and `termio` (not mine). Must land as one change with those areas or DECSCUSR handling doubles up. |

### 2.3 BL-3 safety fixes — **PageList half missing, Screen/Selection half present**

| commit | what changed | status | evidence | guarding test/gate | cost | risk |
|---|---|---|---|---|---|---|
| `e6e4a9fdc` | `Cell.screenPoint` accumulates in `u32` instead of `CellCountInt` (u16) | **ABSENT** | `crates/rustty-vt/src/pagelist/access.rs:1095` `let mut y: size::CellCountInt = self.row_idx;` then `y += Node::rows(node)` in a loop | none | **S** | **Live rustty bug.** In Rust `+=` on `u16` **panics** in debug and **wraps silently** in release once scrollback > 65,535 physical rows. Strictly worse than Zig (which only panics in safety builds). |
| `30b42f42a` | `pointFromPin` uses checked adds and returns `null` on overflow | **ABSENT** | `crates/rustty-vt/src/pagelist/pin.rs:143,148,153` — unchecked `coord.y += …` on `u32` | none | **S** | Same class: debug panic / release wrap on > 2³² rows. |
| `d6e24d985` | Width-aware pin traversal: `PageList.pin` rejects `x >= p.node.cols()`; `leftWrap`/`rightWrap` rewritten as per-row loops using each **destination** page's width; `upOverflow`/`downOverflow` clamp `x` to `@min(self.x, node.cols()-1)` (4 sites) | **ABSENT** | `crates/rustty-vt/src/pagelist/pin.rs:452` `left_wrap` / `:481` `right_wrap` still carry the verbatim pre-fix comment *"This assumes that all pages have the same width"*; `down_overflow` (`:527`) and `up_overflow` (`:577`) copy `x: self.x` unclamped in all four exit paths | none | **M** | Needs the `mixedWidthPinListForTest` fixture (uses `pauseIntegrityChecks` + manual `createPage`) — rustty has both primitives. |
| `0ff4e41b2` | `leftWrap`/`rightWrap` row/col math rebased on `remainder − 1` | **ABSENT** — and rustty is at the **pre-`0ff4e41b2`** formula | `pin.rs:464` `1 + extra_after_remaining / cols`; `pin.rs:468` `cols - extra_after_remaining % cols`; `pin.rs:494` `extra_after_remaining % cols - 1` | none | **S** | **Live rustty crash.** `right_wrap(n)` where `extra_after_remaining % cols == 0` computes `0usize - 1` → **"attempt to subtract with overflow" panic in debug**, `usize::MAX` cast to `u16` in release → out-of-bounds `x`. `left_wrap` symmetrically yields `x == cols` (one past the row). Reproduces on a plain 10×3 list via `right_wrap(14)` from `x=5`. **Highest-severity single finding in this area.** |
| `9b466e9c5` | `PageIterator` `.count` limit: count the current row in both directions, consume the returned length, only advance when rows remain; reverse chunk becomes `start = available-len, end = available` | **ABSENT** — verbatim pre-fix | `crates/rustty-vt/src/pagelist/iter.rs:382–399` (forward: dead `if (len as usize) > *limit`, so it never crosses a page) and `:476–491` (reverse: `start: row.y - len`, `end: row.y - 1`) | none | **S** | **Live rustty crash:** reverse `.count` at `row.y == 0` computes `row.y - 1` on `u16` → debug panic / wrap. Forward `.count` silently never crosses a page boundary (the upstream "impossible condition" bug, faithfully reproduced). |
| `c753fe4a4` | `PageList.scroll(.delta_row)` uses `@abs(n)` instead of `-n` | **ABSENT** | `crates/rustty-vt/src/pagelist/scroll.rs:253,257,300` — `(-n) as usize` | none | **S** | **Live rustty panic** on `isize::MIN` ("attempt to negate with overflow"). Use `n.unsigned_abs()`. |
| `afbf5ba15` | `scrollPrompt` uses `@abs(delta)` | **ABSENT** | `crates/rustty-vt/src/pagelist/scroll.rs:346` `(if delta > 0 { delta } else { -delta }) as usize` | none | **S** | same. |
| `b287f6d1a` | `Terminal.print`: the stored-grapheme walk must *not* assert "no break"; feed breaks into the state machine so mode-2027 can be enabled after zero-width codepoints were attached. Also removes the eraseDisplay workaround in `testPrintSliceDifferential` | **ABSENT** | `crates/rustty-vt/src/terminal.rs:968–970` still `let brk = graphemeBreak(...); debug_assert!(!brk);` | none | **S** | **Live rustty debug panic** on: mode 2027 off → print `a` + U+200B → mode 2027 on → print U+0301. Fix = delete `debug_assert!(!brk)` (keep the call, it mutates `state` — the existing PORT NOTE already documents that). |
| `e727a3658` | `clearRows` gets cells via `page.getCells(row)` (stored page width) not `self.pages.cols`; `clearCells` compares `cells.len == page.size.cols` not `self.pages.cols` (2 sites) | **ABSENT** | `crates/rustty-vt/src/screen.rs:2003` `let cols = self.pages.cols;` → `slice_from_raw_parts_mut(cells_multi, cols)`; `screen.rs:2124` and `:2138` compare `cells_len == self.pages.cols as usize` | none | **S/M** | Trips `clear_cells`' own `debug_assert!` bounds checks on a narrower stored page during incomplete reflow. |
| `a55850c98` | `cursorCellEndOfPrev` uses `page_pin.node.cols()-1` | **ABSENT** | `crates/rustty-vt/src/screen.rs:1197` `page_pin.x = self.pages.cols - 1;` | none | **S** | out-of-bounds cell resolution during incomplete reflow. |
| `607160657` | `Screen.clone` clamps fallback selection pins to the first/last **cloned node's** width | **PORTED** | `crates/rustty-vt/src/screen.rs:997–1035` (`core::cmp::min(ordered.tl.x, cols-1)`, `min(ordered.br.x, cols-1)`, `cols-1`) | `screen.rs` clone tests | — | (uncited by hash — found by reading the code) |
| `fa8cae88b` | `selectLine` end pin uses `prev.node.cols()-1` | **PORTED** | `crates/rustty-vt/src/screen.rs:3614` | `screen.rs` selectLine tests | — | uncited by hash |
| `a9f5b7eba` | `containedRowCached` uses owning page width / clamps rect bounds | **PORTED** | `crates/rustty-vt/src/selection.rs:372` +26/+34/+54/+87 (all four sites) | `selection.rs` containedRow tests | — | uncited by hash |
| `b6f34be44` | `topLeft`/`bottomRight` clamp mirrored columns | **PORTED** | `crates/rustty-vt/src/selection.rs:177` and `:198` (all four arms) | `selection.rs:1098` `top_left`, `:1134` `bottom_right` | — | uncited by hash |
| `0c299000f` | `Screen.select` releases only old pins not owned by the replacement | **PORTED** | `crates/rustty-vt/src/screen.rs:3352–3390` | `screen.rs` select tests | — | uncited by hash |
| `cfce1cd56` | `startHyperlinkOnce` dupes before `endHyperlink` | **PORTED** | `crates/rustty-vt/src/screen.rs:3166–3184` (dupe → `end_hyperlink()` after) | `StartHyperlinkFp::DupeLink` tripwire fail point | — | uncited by hash |
| `0cb004734` | `clearCells` early-returns on empty slice | **PORTED** | `crates/rustty-vt/src/screen.rs:2069` with `PORT NOTE (parity 0cb004734)` | — | — | — |
| `0aaedf436` | `setCursorPos` saturating origin adds | **PORTED** | `crates/rustty-vt/src/terminal.rs:2596` with `PORT NOTE (parity 0aaedf436)` | — | — | — |
| `3d08161b5` | empty tabstop ranges | **PORTED** | `crates/rustty-vt/src/tabstops.rs:313`, test at `:471` | yes | — | — |
| `91f0cf67d` | tabstops preserved on resize failure | **PORTED** | `crates/rustty-vt/src/terminal.rs:4579` | `terminal_resize_preserves_tabstops_on_allocation_failure` | — | — |
| `04810273a` | wrap implicit hyperlink identifiers | **PORTED** | `crates/rustty-vt/src/screen.rs:483` | FailingAllocator hunk noted as adapted | — | — |
| `fdd255c78`, `1d5b6f90e` | atomic title/pwd terminator reservation | **PORTED** | `crates/rustty-vt/src/terminal.rs:5244` | adapted (FailingAllocator → tripwire) | — | — |
| `dace6d1c6`, `a14a619ce` | aliased title/pwd updates (memmove not memcpy) | **PORTED** | `crates/rustty-vt/src/terminal.rs:4678`, `:4771` — `core::ptr::copy` via raw `*mut Vec<u8>`, with a documented Miri finding | Miri-proven per the PORT NOTE | — | exemplary handling of the aliasing landmine. |
| `5caf20e3e` | `Tabstops.resize`: `size <= self.dynamic_stops.len` (was `<`) so an exactly-sized buffer isn't reallocated | **ABSENT** | `crates/rustty-vt/src/tabstops.rs:252` still `if size < self.dynamic_stops.len()` | none — needs the fixed-buffer no-second-allocation test | **S** | pure perf/allocation; no correctness risk. |
| `8307349ec` | `increaseCapacity` growth from a **zero** capacity dimension jumps to the standard default instead of doubling 0 | **ABSENT** | `crates/rustty-vt/src/pagelist/grow.rs:495` `adjust_field!` does `(old as $int).checked_mul(2)` with **no `old == 0` guard** | none | **S** | **Reachable in rustty today** — the precondition is `compact` producing exact capacities, and rustty has both `compact` (`pagelist/grow.rs:610`) and `exact_row_capacity` (`page_impl.rs:898`). Unbounded retry callers (`start_hyperlink`, `cursor_set_hyperlink`, reflow probes, `insert_lines`/`delete_lines`) would **infinite-loop, reallocating a page per iteration**. Single-retry callers (styles, graphemes) would silently drop data. Cheap fix, severe consequence. |

### 2.4 Page maps / probe bounds (`page.zig` + `Screen.zig` sides only)

| commit | what changed (my side) | status | evidence | guarding test/gate | cost | risk |
|---|---|---|---|---|---|---|
| `7e14347c1` | `page.zig`: `GraphemeMap` gains explicit `hash_map.default_max_load_percentage`; `hyperlinkCapacity()` returns `map.maxLoad()` not `capacity()`; `layout()` drops the pre-load-factor `ceilPowerOfTwoAssert` on `hyperlink_map_count`; `exactRowCapacity` comment/derivation updated. `Screen.zig`: comment on the `cursorSetHyperlink` grow path | **ABSENT** | `grep -n "default_max_load_percentage\|max_load" crates/rustty-vt/src/{hash_map,page,page_impl}.rs` → **0 hits**; `crates/rustty-vt/src/page_impl.rs:2218` `hyperlink_capacity` returns `.capacity()` | none | **M** (my side) | **Blocked on the `hash_map.zig` area** — `maxLoad()`/`default_max_load_percentage` must exist there first. Coordinate. Note `fedd42e8d` later *deleted* the headroom counter + in-place rehash, so **do not port `7e14347c1`'s hash_map internals; port its end state**. My side's three `page.zig` hunks survive to `main` (verified at `origin/main:src/terminal/page.zig:98,1501,1769`). |
| `65f953e8e` | `page.zig`: `moveHyperlink`/`moveGrapheme` use `putAssumeCapacityNoClobber` for known-absent destinations | **ABSENT** | `crates/rustty-vt/src/page_impl.rs:2202` and `:2404` both call `put_assume_capacity_context`. rustty *does* have `put_assume_capacity_no_clobber_context` (`hash_map.rs:535`), so this is a 2-line change | none | **S** | trivially portable once the no-clobber semantics match the post-`fedd42e8d` map. |
| `fedd42e8d` | `Screen.zig`: comment rewrite only (backward-shift deletion replaces tombstones) | **ABSENT** (comment) | — | — | **S** | the substance is `hash_map.zig` (another area). Flag only. |

### 2.5 Page memory reclamation (`mem.zig` / pool)

| commit | what changed | status | evidence | guarding test/gate | cost | risk |
|---|---|---|---|---|---|---|
| `f6fd4cb08` | extracts `src/terminal/mem.zig` (`DecommitMode{zero,strict}`, `canReclaim`, `decommit`, `recommit`) and rewires `PageList` | **PORTED** | `crates/rustty-vt/src/mem.rs` (338 lines): `DecommitMode` `:33`, `can_reclaim` `:97`, `decommit` `:154`, `recommit` `:249`; `pagelist/mod.rs:570` calls `crate::mem::recommit` | `mem.rs:290–336` unit tests | — | — |
| `896aca499` | pool items decommitted on `destroyNode`, recommitted on create (`madvise DONTNEED` / `FREE_REUSABLE`+`FREE_REUSE`) | **PORTED** | `crates/rustty-vt/src/pagelist/mod.rs:2410` `decommit_pool_item`, `:2430` `recommit_pool_item`; call sites `:1510`, `:1933`, `:2053` | RSS probe example `crates/rustty-vt/examples/rss_probe.rs`; the perf baseline records RSS wins | — | — |
| `c442eb4b3` | documents + **guards** the pool-reuse zeroing invariant: `comptime assert(@sizeOf(Row) >= @sizeOf(usize))` in `Page`, runtime `assert(cap.rows > 0)` in `Page.initBuf`, comments at `layout`/`createPageExt`; stale doc fixes | **PARTIAL** | the comments/rewiring landed with the family, but **both asserts are missing**: `crates/rustty-vt/src/page.rs:1225` has only the `size_of::<Row>() % align_of::<Cell>()` assert (pre-existing, unrelated); `crates/rustty-vt/src/page_impl.rs:414` `init_buf` has **no `assert!(cap.rows > 0)`** | none | **S** | see §3 — this is the one previously-believed-ported item that is incomplete. |

### 2.6 Perf / render-state / printSlice

| commit | what changed | status | evidence | guarding test/gate | cost | risk |
|---|---|---|---|---|---|---|
| `47e26df60` | **New** `Terminal.printSlice` (`pub`), `printSliceFast`, `printSliceFill` — batched printed codepoint runs into direct row fills (+675 lines in `Terminal.zig`) | **ABSENT** | `grep -n "print_slice" crates/rustty-vt/src/terminal.rs` → **0 hits**. Confirmed new-in-range: `git show 4749c4e93:src/terminal/Terminal.zig | grep printSlice` → nothing; `origin/main` has it at line 511 | none | **L** | Also touches `stream.zig` (+49) and `stream_terminal.zig` (+1) — not my area. Coordinate with the `stream*` area. |
| `cb2d78587` | `printSliceFill` fills style-only cell runs in bulk (+112) | **ABSENT** | depends on `47e26df60` | none | **M** | strictly after `47e26df60`. |
| `f2a7652ab` | (my file) explicit `@Vector` for identical-cell run stores and printable-cell scanning in `printSliceFill`, because Zig 0.16 stopped auto-vectorizing | **ABSENT** / partly **N/A** | inside `printSliceFill` | none | **M** | The *reason* is a Zig codegen regression, so the Rust port should decide independently whether to use `core::simd`/portable-simd or plain loops. Do **not** treat "explicit @Vector" as a fidelity requirement — treat the *semantics* as the requirement. |
| `446f80f4e` | (my files) `page.zig`: `Row.Backing`/`Cell.Backing` backing-integer consts + a comptime-generated **`Mask(T, fields, group_len)`** helper (`bits`, `pattern`, `match`, `matchScalar`, `eql`, `eqlExact`, `eqlScalar`, `eqlPrefix`, `strip`) for masked/SIMD compares (+274). `Terminal.zig`: `printSliceFill` rewritten onto `Mask` | **ABSENT** | `grep -n "Row::Backing\|Cell::Backing\|struct Mask" crates/rustty-vt/src/page*.rs` → 0. rustty has an unrelated `fn mask(bits:u32)->u64` at `page.rs:524` and hand-written `PERF(port)` masked stores (`page.rs:792`, `page_impl.rs:1327,1441`) | none | **L** | `render.zig` (+919, **not my area**) is the other big `Mask` consumer, so the `page.zig` half of this is a **shared dependency**: the render area cannot land its part without it. Recommend `page.Mask` be ported **once, in area A**, and the render area consume it. Rust equivalent needs const-generic field-mask consts rather than comptime reflection — no exotic unsafe required. |
| `77190bd02` | New `Screen.cursorScrollRegionUp` (+ private `cursorScrollRegionUpSlow`) — fast in-page scroll-region rotate with `fastmem.rotateOnce`, zero-fill hot path, viewport-offset and tracked-pin fixups; `Terminal` scroll-region call sites rewired (+184) | **ABSENT** | `grep -n "cursor_scroll_region_up\|rotate_once" crates/rustty-vt/src/{screen,terminal}.rs` → **0 hits** | none | **L** | **Heavy §5 landmine**: the fast path holds a `*mut Page` + a `rows` slice while mutating tracked pins and `self.cursor.page_row`/`page_cell` through other paths, then `assertIntegrity()` re-walks the same memory. Must be raw-pointer-argument style throughout; prove under Miri. |
| `8d663a76e` | `clearCells` groups styled cells into runs and calls `styles.releaseMultiple(id, n)` once per run | **ABSENT** | `crates/rustty-vt/src/screen.rs:2145–2154` still releases per cell. rustty **already has** `release_multiple` (`ref_counted_set.rs:824`), so this is a ~15-line loop rewrite | none | **S** | pure perf; low risk; good early win. |
| `cee35cabf` | skip style-map update when SGR leaves the style unchanged | **PORTED** | `crates/rustty-vt/src/screen.rs:2806` `PORT NOTE (parity cee35cabf)` | — | — | — |

### 2.7 Already-ported remainder (evidence-backed)

| commit(s) | status | evidence |
|---|---|---|
| `16e4b5e98` track page ownership on nodes | **PORTED** | `crates/rustty-vt/src/pagelist/mod.rs:154` `Node.owned`, `:160` `Node::Owned`, `:342` `CreatePage` — all tagged `PORTS: … (16e4b5e98)` |
| `f6d9a582e` serial floor → epoch | **PORTED** | `pagelist/mod.rs:1053` `page_serial_epoch` (tagged `f6d9a582e`), integrity check at `:1648` |
| `f1a5fab45` conservative serial floor | **PORTED** | `pagelist/tests.rs:2026`, `:2504`, `:2546` |
| `dd956f37c` renew generations for shifted rows | **PORTED** | `pagelist/tests.rs:2051`, `:2090`; `pagelist/resize.rs:1070` |
| `0bad91adb` renew generations for direct row shifts | **PORTED** | `terminal.rs:2990` + `invalidate_full_width_row_range` at `:2996`, called `:3124`; tests `terminal.rs:10333,12268,12695,12868`, `screen.rs:11634,11731,11946` |
| `d0a26d146` reschedule compression after page replacement | **PORTED** | `pagelist/tests.rs:2357` |
| `91a63e528` invalidate split source page refs | **PORTED** | `pagelist/tests.rs:2392` |
| `a804e3ac7` invalidate partially erased page refs | **PORTED** | `pagelist/tests.rs:2425`, `:2456` |
| `421fe8dab`, `9156ada16`, `70e788f06`, `e7969ed43`, `f8c217e55`, `181254d36`, `95685afd2` (BL-1 core) | **PORTED (hook/symbol level — see coverage note)** | `crates/rustty-vt/src/pagelist/compress.rs` (`activity_serial` `:132`, `last_serial` `:137`, `next_serial` `:141`, `ACTIVITY_SERIAL_MASK` `:159`, `compress` `:485`, `compress_incremental` `:522`, `memory_stats` `:623`); all **9** non-test `markActivity` sites present in the correct functions (`grow.rs:105` split, `:293` grow, `:576` increase_capacity, `:666` compact, `scroll.rs:87` scroll, `erase.rs:41/187/485` erase_row/erase_row_bounded/erase_rows, `resize.rs:151` resize) — matches upstream `PageList.zig:1232,2834,3236,3312,3533,3840,4464,4582,4809` exactly |
| `172f15da3` expose compression through lib-vt (my side) | **PORTED** | `terminal.rs:348` `TerminalCompressionMode`, `:361` `TerminalCompressionResult`, `:2861` `compress`; the moved `can_reclaim(.strict)` guard is at `pagelist/compress.rs:504` in the `Full` arm (upstream removed it from `compressFull` and added it to `compress`) — **exact match** |
| `e44f5cb0f` zero-capacity `RefCountedSet` guard | **PORTED** | `ref_counted_set.rs:929`, test `style.rs:1536` |
| `b953bb346` bitmap allocator chunk region sizing | **PORTED** | `bitmap_allocator.rs:660` |
| `14c829883` (my side) `colorForXterm`/`colorForKitty` | **PORTED** | `terminal.rs:697`, `:722` |
| `65e61282a` (my side) `graphemeWidthEffect` in `print` | **PORTED** | `terminal.rs:988`, parity test `:5535` |
| `3a2e28329` scroll-to-row viewport scrolling | **PORTED** | 9 tests `pagelist/tests.rs:3846–4270` |
| `5fa37f2b7` `usize` remaining-rows in `scroll` | **PORTED** | `pagelist/scroll.rs:160`, `:191` |
| `86e1f7b8b` reject replaced result pages | **N/A (search area)** | `search/*` is explicitly not mine |
| `e8525c0fd` Zig 0.16.0 | **N/A (mechanical) — sampled** | see coverage note |
| `style.zig` `content.color_palette` → `.color_palette.data` | **N/A** | Zig-0.16 packed-union padding requirement; rustty's `Cell` content is a Rust enum/bitfield with no analogue |
| `6f10ddfe8`, `739603b8a` | **N/A** | merge commits, no unique diff |
| `cb2fef390` DECRQSS preserves underline style | **ABSENT** | `crates/rustty-vt/src/terminal.rs:4379` `print_attributes` emits a bare `b'4'` (line +61 of that fn) with no `;4:{n}` sub-parameter branch. Fix: on `'4'` with `pen.flags.underline != Single`, write `;4:{n}` and continue. **S**, no risk. Needs the 5-case table test. |
| `6c8c07981` visibility reports (my side: `Terminal.zig` +11) | **ABSENT** | `grep -rn "2033\|visibility" crates/rustty-vt/src` → no mode 2033, no visibility state | **S** (my side) | cross-area: `device_status.zig` (+34), `modes.zig` (+1), `stream_terminal.zig` (+69), `termio/*`, `Surface.zig`, `include/ghostty/vt/modes.h`. My part is the `Terminal` state field + reset exemption ("view visibility survives RIS"). |
| `src/datastruct/segmented_list.zig` (NEW, +533) | **N/A for area A** | **Nothing in my area consumes it.** Upstream consumers are `src/datastruct/segmented_pool.zig`, `src/font/Collection.zig`, `src/terminal/osc/parsers/color.zig` (`git grep -l segmented_list origin/main`). rustty has **no** `segmented_list.rs` (`crates/rustty-datastruct/src/` lacks it) and `crates/rustty-vt/src/osc/parsers/color.rs:417` documents that `std.SegmentedList` was deliberately modeled differently. **Flag for the datastruct/osc/font areas**, not mine. |

---

## 3. THE PARTIALS — read this section first

### 3.1 `c442eb4b3` — the only true "believed ported, actually incomplete" item

The BL-2 page-memory family (`f6fd4cb08`, `896aca499`, `c442eb4b3`) is treated as
shipped and the first two genuinely are. `c442eb4b3`'s **guards are missing**. Exact
missing hunks:

```zig
// upstream src/terminal/page.zig, inside Page's comptime block (origin/main:160)
assert(@sizeOf(Row) >= @sizeOf(usize));
```
→ rustty `crates/rustty-vt/src/page.rs:1225` has only the unrelated
`size_of::<Row>() % align_of::<Cell>()` assert. Needs a `const _: () = assert!(...)`
(or `debug_assert!` in the same block) for `size_of::<Row>() >= size_of::<usize>()`.

```zig
// upstream src/terminal/page.zig, Page.initBuf (origin/main:259)
assert(cap.rows > 0);
```
→ rustty `crates/rustty-vt/src/page_impl.rs:414` `init_buf` has no such assert.

**Why it matters in rustty specifically.** `decommit_pool_item`
(`pagelist/mod.rs:2410`) is live, so rustty already relies on the invariant these
asserts protect: pool free-list nodes occupy the first pointer-size bytes of a
recycled page buffer, and only `init_buf` rewriting row 0 erases them. rustty
**skips the zeroing memset in release builds** exactly as Zig does. A future layout
reorder (rows not at offset 0) or a zero-row page would corrupt pages **in release
builds only**, non-deterministically depending on pool reuse. Two asserts. Port them.

### 3.2 The BL-3 safety series is a **half-ported family**, which reads as done

This is the most valuable structural finding. `.port/phase-2/MAIN_PARITY.md` and the
in-source hash citations make BL-3 look landed. In reality the split is by *file*:

- **`Screen.zig` / `Selection.zig` / `Tabstops.zig` / `Terminal.zig` fixes: 12 of 13 ported.**
- **`PageList.zig` fixes: 0 of 7 ported.** (`e6e4a9fdc`, `30b42f42a`, `d6e24d985`,
  `0ff4e41b2`, `9b466e9c5`, `c753fe4a4`, `afbf5ba15`)
- Plus 3 stragglers on the ported side of the fence: `b287f6d1a` (Terminal),
  `e727a3658` + `a55850c98` (Screen).

**Four of these are live, reachable crashes in rustty today, and three are worse in
Rust than in Zig** because Rust's arithmetic panics in debug and wraps in release
where Zig only traps in safety builds:

| rustty site | trigger | symptom |
|---|---|---|
| `pagelist/pin.rs:494` (`right_wrap`) | `right_wrap(n)` landing on an exact row multiple, e.g. `x=5`, `n=14`, `cols=10` | `extra % cols - 1` = `0usize - 1` → **debug panic**; release → `usize::MAX` → `x` out of bounds → later OOB cell read |
| `pagelist/pin.rs:468` (`left_wrap`) | symmetric exact multiple | `x == cols` (one past end of row) → OOB on `row_and_cell()` |
| `pagelist/iter.rs:489` (reverse `.count`) | any reverse count-limited iteration starting at `row.y == 0` | `row.y - 1` on `u16` → **debug panic** |
| `pagelist/scroll.rs:253/346` | `scroll(.delta_row = isize::MIN)` / `.delta_prompt = isize::MIN` | **debug panic** "attempt to negate with overflow" |
| `terminal.rs:970` | mode 2027 off → print base + U+200B → mode 2027 on → print U+0301 | **debug panic** on `debug_assert!(!brk)` |
| `pagelist/grow.rs:495` | `increase_capacity` on a `compact`ed page (zero style/grapheme/string/hyperlink capacity) | **infinite loop** allocating a page per iteration in `start_hyperlink`/`cursor_set_hyperlink`/reflow probes/`insert_lines`/`delete_lines`; silent data loss in the single-retry style/grapheme paths |
| `pagelist/access.rs:1095` | scrollback > 65,535 physical rows, `Cell::screen_point()` | debug panic / **silent wrap** in release |
| `pagelist/pin.rs:143` | scrollback > 2³² rows | debug panic / silent wrap |

Recommendation: land these as **one small, high-value first wave** (all S/M, no
cross-area coupling, no signature changes) before any of the large features.

---

## 4. Ordering and dependencies

### Recommended landing order

**Wave A1 — BL-3 completion (S/M, self-contained, highest value/cost ratio).**
`0ff4e41b2` → `d6e24d985` (in that order; `d6e24d985` rewrites the same functions
`0ff4e41b2` fixes, so **it is legitimate to port only `d6e24d985`'s end state** and
carry `0ff4e41b2`'s regression tests) · `9b466e9c5` · `c753fe4a4` · `afbf5ba15` ·
`e6e4a9fdc` · `30b42f42a` · `b287f6d1a` · `e727a3658` · `a55850c98` · `8307349ec` ·
`c442eb4b3` asserts · `5caf20e3e` · `cb2fef390` · `8d663a76e`.
No dependencies on any other area. **Do this first.**

**Wave A2 — page-map / capacity plumbing.**
`65f953e8e` (2 lines, needs nothing) · then `7e14347c1`'s `page.zig` side —
**blocked on the `hash_map.zig` area** shipping `maxLoad()` /
`default_max_load_percentage` in its post-`fedd42e8d` (backward-shift-deletion) end
state. Do **not** port `7e14347c1`'s tombstone-headroom machinery; `fedd42e8d`
deletes it.

**Wave A3 — capacity-error resilience.**
`ee9d5b352` (adds `IncreaseCapacity::for_clone_error` + `clone_row_grow_capacity` +
`Screen::clone_partial_row_grow_capacity`) → `043326249` (reuses the same mapping).
`043326249` must come second so it can share the helper.

**Wave A4 — resize.**
`dde3d4d6b` (Screen) → `89b103dd5` (Terminal `Resize` struct + features) →
`a3c1caba5` (Terminal rollback) → `b988efcfe` folded in. `89b103dd5` changes
`Terminal::resize`'s signature, so it must be **coordinated in one landing** with
`crates/rustty-vt-c` (`c/terminal.zig` is −49/+… upstream), `rustty-termio`, and
`rustty-core`. `c594031d5` (cursor defaults) is independent of resize but must land
**atomically with the `stream_terminal` and `termio` areas**, which *delete* the code
it absorbs.

**Wave A5 — scrollback limits (biggest ripple).**
`5b2d3b7df` → `86f81fb5b` → `10bc43420` → `f4c68d65e`. Strict order: `f4c68d65e`
refactors `5b2d3b7df`'s output into `Limits` and deletes `maxSize`/`minMaxSize`/
`enforceMaxLines`. Two mechanical mega-renames must each land as a single atomic
commit:
- `PageList::init(cols, rows, max_size)` → `init(Options)` — **256 call sites, 8 files**.
- `max_scrollback` → `max_scrollback_bytes` (+ new `max_scrollback_lines`) — **204
  occurrences, 45 files, 9 crates**.

Land Wave A5 **after** A1–A4 or the renames will conflict with every other change.

**Wave A6 — perf / printSlice / Mask.**
`446f80f4e`'s **`page.zig` `Mask` half must land in area A first**, because
`render.zig` (+919, other area) is its other major consumer. Then `47e26df60` →
`cb2d78587` → `f2a7652ab`'s vectorization → `446f80f4e`'s `printSliceFill` rewrite.
`77190bd02` (`cursorScrollRegionUp`) is independent of the printSlice chain and can
run in parallel, but is the heaviest Miri risk in the area.

**Independent, any time:** `6c8c07981` (visibility) — my `Terminal.zig` part is ~11
lines, but the feature only becomes observable once `modes.zig`, `device_status.zig`,
`stream_terminal.zig`, `termio` and `Surface` land theirs.

### Files another area also touches (coordinate explicitly)

| change | also lands in | owner |
|---|---|---|
| `89b103dd5`, `a3c1caba5` | `stream_terminal.zig`, `c/terminal.zig`, `termio/*` | stream / c-API / termio |
| `c594031d5` | `stream_terminal.zig` (−33), `termio/stream_handler.zig` (−68), `c/terminal.zig` | stream / termio / c-API |
| `47e26df60` | `stream.zig` (+49), `stream_terminal.zig` | stream |
| `446f80f4e` | `render.zig` (+919), `c/render.zig`, `include/ghostty/vt/render.h` | render / c-API |
| `7e14347c1`, `fedd42e8d`, `65f953e8e` | `hash_map.zig` (+601 net), `hyperlink.zig`, `benchmark/HyperlinkMap.zig` | hash_map / bench |
| `6c8c07981` | `modes.zig`, `device_status.zig`, `stream_terminal.zig`, `termio/*`, `Surface.zig`, `include/` | modes / stream / termio / apprt |
| `172f15da3`, BL-1 | `compress/*`, `c/terminal.zig` | **area B — actively being edited concurrently** |
| `86f81fb5b` rename | 45 files across 9 crates | everyone; treat as a workspace-wide rename commit |
| `segmented_list.zig` | `datastruct`, `osc/parsers/color.zig`, `font/Collection.zig` | datastruct / osc / font |
| `f4c68d65e` | `inspector/widgets/pagelist.zig`, `inspector/widgets/screen.zig` | inspector |

---

## 5. Rustty-specific landmines, per hunk

**(a) Stacked Borrows: `&mut Page` / `&mut Row` / `&mut [Cell]` / `&mut Screen` args
re-read across a mutating call.** The established convention here is raw-pointer args
with narrowly-scoped `&mut` reconstruction (see the exemplary PORT NOTE at
`crates/rustty-vt/src/screen.rs:2055–2068`). Upstream hunks that will hit this:

| hunk | why |
|---|---|
| `77190bd02` `cursorScrollRegionUp` | holds `page: *Page` + a `rows` slice, mutates `tracked_pins` and `self.cursor.page_row`/`page_cell` through *other* paths, then `assertIntegrity()` re-walks the same page. **Highest risk in the area.** `fastmem.rotateOnce(Row, rows)` must not be `&mut [Row]` while pins into those rows are live. |
| `dde3d4d6b` `Screen.resize` | holds `cursor_node`/`cursor_node_serial` and extra style+hyperlink refs across `PageList::resize`, which reallocates pages; the `errdefer` restore then writes back through `self.cursor`. Raw pointers throughout; do not keep a `&mut Page` across the `pages.resize` call. |
| `ee9d5b352` `cloneRowGrowCapacity` / `043326249` | the doc comment is explicit: "The source must NOT be on the given node since the node's page memory may be freed on capacity increase." Two raw `*mut Page` that must be proven disjoint; `increase_capacity` frees one of them. |
| `e727a3658` `clearRows`/`clearCells` | already raw-pointer style in rustty; the change (use `page.get_cells(row)` and `page.size.cols`) must keep it that way — do not "simplify" to `&mut [Cell]`. |
| `8d663a76e` run-grouped style release | reads `cells[j].style_id` while calling `page.styles.release_multiple(...)`, which mutates page memory. Read the ids through the raw `*mut Cell` (as the existing loop does), never through a live `&mut [Cell]`. |
| `446f80f4e` `Mask` | `eqlPrefix` takes `cells[0..cell_count]` and indexes from an offset; in Rust prefer `*const [Cell]` + `ptr::read` over `&[Cell]`, since callers hold `*mut Page` aliases to the same memory. |
| `f4c68d65e` `Limits.enforce(self: *const Limits, pagelist: *PageList, key)` | Zig passes a `*const Limits` that **lives inside the `*PageList` it mutates** — a self-alias. In Rust this cannot be `(&self, &mut PageList)`. Port as an inherent `PageList` method that copies the `Limits` value first, or take `*mut PageList`. **Do not** mirror the Zig signature. |
| `5b2d3b7df` `enforceMaxLines` | iterates `tracked_pins.keys()` setting `p.garbage = true` while `erasePage` remaps the same pins. Pins are already raw `*mut Pin` in rustty; keep it that way. |

**(b) Zig byte-slices → `Vec<u8>`/`&[u8]`, never lossy `String`.** No hunk in this
area introduces a new byte-slice API. `dace6d1c6`/`a14a619ce` (title/pwd) already got
this right (`terminal.rs:4678`, `:4771`, `core::ptr::copy` for aliasing).
`cb2fef390` writes `";4:{n}"` into a `[]u8` buffer — keep `print_attributes`' existing
`&mut [u8]` + returned `&[u8]`; do not format through `String`.

**(c) Zig `undefined` has no sound Rust analogue — do not reach for `MaybeUninit`.**
Two upstream *test* hunks construct `Node{ .data = .{ .resident = undefined } }` and
`Cell{ .row = undefined, .cell = undefined }` to build metadata-only page graphs
without allocating cells (`30b42f42a`, `e6e4a9fdc`, `d6e24d985`'s
`mixedWidthPinListForTest`). **Do not port these as `MaybeUninit`.** Either build a
real (tiny, 1-col) page via `create_page` as `mixedWidthPinListForTest` itself does
for the mixed-width case, or add a zeroed test-only constructor. Note
`d6e24d985`'s fixture deliberately calls `pause_integrity_checks(true)` through
`deinit` — rustty has that primitive (`pagelist/mod.rs`), so the fixture is portable.

**(d) Rust arithmetic is *stricter* than Zig's in debug and *laxer* in release.** Every
`@abs` / `+|` / `-|` / `std.math.add(...) catch` hunk in §2.3 must become an explicit
`unsigned_abs()` / `saturating_add` / `saturating_sub` / `checked_add`. A plain `+=`
on `u16`/`u32`/`usize` is **not** a faithful port of Zig's wrapping-free arithmetic:
it panics in debug and wraps in release. The four live crashes in §3.2 are all
instances of this.

**(e) `comptime` reflection.** `446f80f4e`'s `Mask(T, fields, group_len)` and
`fieldMask` use `@typeInfo`/`@FieldType` over packed structs; `8307349ec` uses
`inline else |tag|` over `Capacity`'s fields, which rustty already expands manually
via the `adjust_field!` macro (`pagelist/grow.rs:492`). Follow that precedent: hand-
expanded per-field consts + a macro, not a trait-object abstraction.

**(f) `f2a7652ab`'s `@Vector` is a workaround for a Zig 0.16 codegen regression**, not
a semantic requirement. rustty must decide independently (portable `core::simd` vs.
scalar loop that LLVM auto-vectorizes). Do not treat the vector form as R1-faithful
behaviour to reproduce.

---

## 6. Upstream deletions vs. rustty

Symbol-level inventory diff, pin → `origin/main`, cross-checked against rustty:

| upstream file | removed `pub fn` | removed private `fn` | rustty still has it? | must rustty delete? |
|---|---|---|---|---|
| `PageList.zig` | `maxSize` | `minMaxSize` | **yes** — `pagelist/mod.rs:2175` `pub fn max_size()`, `mod.rs:1254` `fn min_max_size()` | **Yes, as part of `f4c68d65e`.** Both fold into `Limits.max(.bytes)` / `Limits.Limit.min`. Only 3 in-crate `max_size()` call sites, so cheap. Not a semantic deletion — a rename/relocation. |
| `Screen.zig` | none | none | — | — |
| `Terminal.zig` | none | none | — | — |
| `page.zig` | none | none | — | — |
| `Selection.zig`, `Tabstops.zig`, `style.zig`, `ref_counted_set.zig`, `bitmap_allocator.zig`, `StringMap.zig`, `ScreenSet.zig` | none | none | — | — |

Also *renamed away* (not deleted): `Screen.Options.max_scrollback` → `max_scrollback_bytes`
and `Terminal.Options.max_scrollback` → `max_scrollback_bytes`. rustty must follow
(§2.1 / Wave A5).

**No behaviour was removed from area A in this range that rustty must drop.** The only
deletions are refactoring artefacts (`maxSize`/`minMaxSize` into `Limits`) and
`7e14347c1`'s tombstone-headroom machinery, which never existed in rustty and must
**not** be introduced (see Wave A2).

New symbols upstream, for completeness (all absent in rustty except as noted):
`PageList`: `compress`✓(ported), `invalidateNodeLayout`✓, `memoryStats`✓, `nodeIsValid`✓,
`setMaxBytes`✗, `setMaxLines`✗, `cloneRowGrowCapacity`✗, `compressFull`✓,
`compressIncremental`✓, `compressPage`✓.
`Screen`: `clonePartialRowGrowCapacity`✗, `cursorScrollRegionUp`✗, `cursorScrollRegionUpSlow`✗,
`clearPromptForRedraw`✗.
`Terminal`: `colorForKitty`✓, `colorForXterm`✓, `compress`✓, `compressionActivity`✓,
`io`(N/A), `printSlice`✗, `printSliceFast`✗, `printSliceFill`✗,
`invalidateFullWidthRowRange`✓, `setCursorStyle`✗, `setDefaultCursorStyle`✗,
`setDefaultCursorBlink`✗, `setScrollbackMaxBytes`✗, `setScrollbackMaxLines`✗.
`page.zig`: `Mask` + `bits`/`eql`/`eqlExact`/`eqlPrefix`/`eqlScalar`/`match`/
`matchScalar`/`pattern`/`strip` — all ✗.

---

## 7. Open questions for a human, with recommendations

1. **Phase 3 artifacts don't exist on `main`.** `.port/phase-3/README.md`,
   `UPSTREAM_DIFF.patch`, `UPSTREAM_COMMITS.txt` are absent; I derived everything
   from the oracle repo directly.
   **Recommendation:** land the README + artifacts before the port plan, and re-run
   the range derivation to confirm `4749c4e93..6ad1fe7d8`. My commit lists are
   reproducible from the exact `git log` invocations quoted in §1–2.

2. **Ship the four live crashes as a hotfix, ahead of the Phase 3 plan?**
   `right_wrap`/`left_wrap` exact-multiple, reverse `.count` at row 0,
   `isize::MIN` scroll deltas, `increase_capacity` from zero capacity — all are
   reachable on `main` today, all are S, none touch another area.
   **Recommendation: yes.** Land Wave A1's crash subset as its own PR immediately
   with regression tests; don't hold them behind the scrollback-limits refactor.

3. **`d6e24d985` supersedes `0ff4e41b2` in the same functions. Port both, or just
   the end state?**
   **Recommendation: port `d6e24d985`'s end state only**, but carry *both* commits'
   regression tests (`0ff4e41b2`'s two exact-multiple tests, `d6e24d985`'s three
   mixed-width tests). Record both hashes in the PORT NOTE so the parity ledger
   stays honest.

4. **`7e14347c1` → `fedd42e8d` likewise: tombstones-then-backward-shift.** Porting
   `7e14347c1`'s headroom counter and in-place rehash would be ~470 lines of
   `hash_map.zig` that `fedd42e8d` immediately deletes.
   **Recommendation: port the `fedd42e8d` end state directly** in the hash_map area,
   and gate my `page.zig` hunks (`maxLoad`, `default_max_load_percentage`, layout
   double-rounding) on it. Requires an explicit hand-off with that area's owner.

5. **`page.Mask` — who owns it?** Both `Terminal.printSliceFill` (mine) and
   `render.zig` (+919, not mine) consume it, and it needs const-generic field masks
   in Rust rather than Zig comptime reflection.
   **Recommendation: area A ports `page::Mask` first as a standalone, unit-tested
   change**, and the render area consumes it. Otherwise both areas will independently
   hand-roll masked compares and diverge.

6. **`f2a7652ab`'s explicit `@Vector`: fidelity or codegen workaround?**
   **Recommendation: treat as a workaround, not R1-faithful behaviour.** Port
   `printSliceFill`'s *semantics*, benchmark rustty's scalar version, and only reach
   for `core::simd` if the numbers demand it. Document the decision in the PORT NOTE
   so a future sweep doesn't "fix" it back.

7. **`e8525c0fd` (Zig 0.16) — I sampled ~2,000 of 2,852 lines in my files.**
   **Recommendation:** run one focused mechanical-sweep pass over
   `e8525c0fd -- src/terminal/{PageList,Screen,Terminal}.zig`, grepping for
   non-`std.`/non-`posix.`/non-`@` hunks, before declaring area A closed. Everything
   I read was pure language migration, but I will not claim coverage I don't have.

8. **BL-1 line-level re-vet is not mine and I could not do it safely** — a concurrent
   lane is editing `crates/rustty-vt/src/{compress,pagelist}/` as I write.
   **Recommendation:** area B re-vets the seven BL-1 commits hunk-by-hunk *after*
   that lane lands. My hook-level check (9/9 `markActivity` sites in the right
   functions, `compress`/`Full`-arm `can_reclaim` guard matching `172f15da3`'s end
   state) is a strong positive signal but is **not** a line-level verdict.

9. **Where should the guarding tests come from?** 30 of the 32 absent items carry
   upstream tests, but several use `std.testing.FailingAllocator`, which rustty
   replaces with tripwire fail points (existing precedent: `terminal.rs:5244`,
   `tabstops.rs:334`, `StartHyperlinkFp::DupeLink`).
   **Recommendation:** make "port the upstream test, adapting FailingAllocator →
   tripwire" a hard requirement per row, and add tripwire fail-point modules for
   `Screen::resize` (`saved_cursor_pin`, `pages`) and `Terminal::resize` (`tabstops`,
   `primary_screen`, `alternate_screen`, `alternate_screen_init`) as part of Wave A4 —
   upstream defines exactly those names, so the parity is free.
