# MAIN_PARITY — upstream ghostty parity ledger (R12, T19)

**Pin (oracle reference):** `4749c4e93` (2026-06-20, "i18n(kk): translate missing Close Split string (#13052)") — ancestor of `origin/main`.
**Upstream head at triage:** `origin/main` = `53bd14fec` (fetched 2026-07-10; `git fetch` only — checkout untouched, R3).
**Scope:** `git -C /Users/gael/projects/ghostty log --oneline 4749c4e9..origin/main -- src/ include/` → **129 commits** (116 non-merge + 13 PR-merge commits).

**Doctrine (verbatim, binding):**
> a merge does NOT port the fix; only an explicit .rs change does

> ALSO port the test hunks from the same commit

Updating/fetching the reference checkout ports nothing. Every verdict below is against the rustty working tree as of this triage.

**Verdict legend:**
- `PORTED` — explicit .rs change landed this pass (T19), with the commit's test hunks.
- `PARTIAL` — fix ported; some hunk (usually a test) not portable, reason recorded.
- `missing` — not in rustty; `BL-n` names the backlog entry that owns it.
- `pending-small` — missing AND assessed portable as a small/medium standalone fix (BL-3 queue for the next parity pass).
- `n/a-merge` — PR merge commit; constituents triaged individually.
- `n/a` — no port action possible/meaningful (reverted upstream, or pure Zig decl reordering).
- `n/a-tier` — touches a tier not yet ported (GTK apprt); owned by that tier's port plan (BL-8).

---

## 1. Ported this pass (T19) — 11 commits

| upstream | subject | rustty change (evidence) | test hunks |
|---|---|---|---|
| `b953bb346` | terminal: fix bitmap allocator chunk region sizing | `crates/rustty-vt/src/bitmap_allocator.rs:335` (`aligned_chunk_count * CHUNK_SIZE`) | both: `BitmapAllocator_layout_chunks_region_matches_bitmap_addressable_bytes`, `BitmapAllocator_layout_small_capacity_cannot_alloc_out_of_bounds` |
| `e44f5cb0f` | terminal: guard RefCountedSet lookups against zero-capacity sets | `crates/rustty-vt/src/ref_counted_set.rs:825` (`table_cap == 0` guard) | `style.rs` `set_zero_capacity` (0xFF adjacent-memory probe) |
| `3d08161b5` | terminal: handle empty tabstop ranges | `crates/rustty-vt/src/tabstops.rs:315` (`interval == 0 \|\| cols <= 1` return) | `tabstops_interval_with_zero_columns` |
| `0aaedf436` | terminal: saturate origin cursor offsets | `crates/rustty-vt/src/terminal.rs:2495` (`saturating_add` on both origin offsets) | `terminal_set_cursor_pos_saturates_overflowing_origin_offsets` |
| `0cb004734` | terminal: ignore empty cell clears | `crates/rustty-vt/src/screen.rs:2017` (`cells.len() == 0` guard before integrity pause) | `screen_clear_cells_empty_range` |
| `91f0cf67d` | terminal: preserve tabstops on resize failure | `crates/rustty-vt/src/terminal.rs:4362` (allocate replacement BEFORE deinit of old) | **PARTIAL** — upstream test uses `std.testing.FailingAllocator`; this port's `Allocator` is the infallible `allocator_api2::Global` handle (terminal.rs:94) with no failure-injection seam on `Tabstops::init` (the tripwire module only instruments `Tabstops::resize`). Test hunk recorded here as debt; portable if/when an injectable allocator seam lands. |
| `5fa37f2b7` | Use correct type for number of remaining rows so that PageList.scroll works with long scrollback history | `crates/rustty-vt/src/pagelist/scroll.rs:135,166` (`rem: usize`; `CellCountInt` cast moved to pin `y` assignment) | none upstream |
| `cee35cabf` | terminal: skip style map update when SGR leaves style unchanged | `crates/rustty-vt/src/screen.rs:2752` (`style.eql(&old_style)` early return) | none upstream |
| `8f1c2fe95` | terminal: handle backwards selection timestamps | `crates/rustty-vt/src/selection_gesture.rs:790` (`time_since -> Option<u64>`, `None` on backwards; press_repeat treats as failed repeat; Press doc updated) | `backwards_repeat_time_resets_click_count` (Instant built from `now()` anchor + Duration — Rust `Instant` has no raw constructor; ordering relationship preserved) |
| `dac341cad` | font/sprite: make cursor height respect adjust-cursor-height | `crates/rustty-font/src/sprite/face.rs:704-716` (cursor sprites use `metrics.cursor_height`; `offset_y` re-centers by `(cell_height - height)/2`) | see `e8f3f6c43` |
| `e8f3f6c43` | font/sprite: add regression test for cursor-height metric | `crates/rustty-font/src/sprite/face.rs` `full_height_cursor_sprites_respect_cursor_height_metric` (18 height/offset assertions, 3 sprites × 3 cursor heights) | is the test |

**Verification:** `cargo test -p rustty-vt --lib` → 2035 passed / 0 failed (all 7 new vt tests listed in run output). `cargo test -p rustty-font --lib` → 75 passed / 0 failed (new test listed).

**Found while porting (pre-existing, pin-era):** none required a fix; `RefCountedSet.count()` initially looked missing but exists at `ref_counted_set.rs:747`.

---

## 2. Backlog entries (structural — do NOT cherry-pick piecemeal)

- **BL-1 — Scrollback page compression + page-generation/search-integrity stack** (~35 commits, PRs #13264/#13274/#13282 + follow-ups). New `src/terminal/compress/` subsystem (LZ4 block codec, compressed `Page` representation, PageList integration, renderer/termio/inspector/config/bench/lib-vt surface) plus the page **generation** machinery and the search fixes built on it. Evidence of absence: no `compress*` module under `crates/rustty-vt/src/` (`ls`), `grep -r generation crates/rustty-vt/src/pagelist` hits only screen-set generations; pin `PageList.zig` has 0 `generation` hits vs post-pin's pervasive use. Port as its own sub-project in upstream commit order (c62c15984 → 53bd14fec); the search fixes in this stack land with it, not before.
- **BL-2 — `896aca499` return free-listed page memory to the OS** (madvise/decommit on the page pool; 172 ins). Standalone but platform-conditional memory work; interacts with BL-1's reclamation generalization (`f6fd4cb08`).
- **BL-3 — pending-small queue** — see §3 table rows marked `pending-small`. Each is a small/medium single-concern fix to pin-era ported code; port assertion-for-assertion WITH its test hunks, one commit per upstream commit, next parity pass.
- **BL-4 — stream/terminal/termio performance series** (#13206-era + later: SIMD ESC/ASCII inline, CSI bulk parse/dispatch, print run batching, scroll-region opts, render-state lock-hold opts, APC bulk dispatch, pty read pipelining, per-run style release, once-per-value logging). Perf-intent work over ported code; several commits interlock (stream fast paths ↔ terminal handlers ↔ bench harness). Port as a series with the bench corpus commits (`4c5b1d5d5`, `258de36d1`).
- **BL-5 — lib-vt C-API expansion** (color utility APIs, color-scheme report encoder, OSC/Kitty color-query reporting, unicode width APIs, scroll-to-row tag, clipboard_set/OSC-52 effects, RGB-by-pointer ABI fix, selection-gesture export). Touches `include/ghostty/vt/*`, `src/terminal/c/*`, `src/lib_vt.zig` → `rustty-vt-c`/`rustty-lib`. ABI-shaped: port as a coordinated batch against the C header contract (G4 byte-parity gates apply). Evidence: `ScrollViewport` at `crates/rustty-vt/src/terminal.rs:318` has no `Row` variant; `grep -r clipboard_set crates/rustty-vt*` → 0 hits.
- **BL-6 — kitty/gfx generation stamps** (`bdc0b6c19` + build fix `8fe5913f7`). Deletes transmit-time in favor of generation stamps across graphics storage/exec/image/renderer + C API. Evidence: `grep -rn transmit_time crates/rustty-vt/src/kitty` → 8 hits (pre-change shape).
- **BL-7 — OSC 72 kitty drag-and-drop parser** (`91c87e2cf` + doc/test follow-ups `dd6c09bcf`, `a6132c18b`, `c488ccda6`). New `osc/parsers/kitty_dnd_protocol.zig`. Evidence: `grep -r kitty_dnd crates/rustty-vt/src` → 0 hits.
- **BL-8 — GTK tier** (`004c88e41`, `0a117e079`, `5ea45919d` + config option docs `b5f5ca078`). GTK apprt is not yet ported (heavy-tier plan: GTK last); these belong to that tier's pre-port commit sweep, including the `gtk-horizontal-tab-scroll` Config option.

---

## 3. Per-commit verdict table (all 129, newest → oldest)

| upstream | subject | verdict | mapping / evidence |
|---|---|---|---|
| `53bd14fec` | terminal: page serial as generation marker (#13282) | n/a-merge | constituents below |
| `89eca063e` | terminal/search: avoid pruning history on reload | missing BL-1 | search/screen.rs; fix is against BL-1 generation machinery |
| `0bad91adb` | terminal: renew generations for direct row shifts | missing BL-1 | pagelist has no page-generation field |
| `d4ac93a03` | clipboard_set effect for OSC 52 (#13182) | n/a-merge | constituents below |
| `634ef7198` | terminal: make clipboard writes protocol neutral | missing BL-5 | no `clipboard.zig` analogue / clipboard_set in rustty-vt(-c) |
| `f6d9a582e` | terminal: rename page serial floor as epoch | missing BL-1 | serial-floor concept lands with BL-1 |
| `dd956f37c` | terminal: renew generations for shifted rows | missing BL-1 | — |
| `d0a26d146` | terminal: reschedule compression after page replacement | missing BL-1 | — |
| `a89c6133c` | terminal/search: validate history before reload | missing BL-1 | — |
| `c9ad708ef` | terminal: validate flattened highlight generations | missing BL-1 | render.rs highlight flattening; generation-based |
| `c243d89bd` | terminal/search: generations in viewport fingerprints | missing BL-1 | — |
| `83aada205` | terminal/search: preserve serial order in reverse matches | missing BL-1 | sliding_window.rs; ordered on BL-1 serial changes |
| `91a63e528` | terminal: invalidate split source page refs | missing BL-1 | — |
| `a804e3ac7` | terminal: invalidate partially erased page refs | missing BL-1 | — |
| `f1a5fab45` | terminal: keep page serial floor conservative | missing BL-1 | — |
| `8c523ed03` | terminal: vectorize APC payload scanning | missing BL-4 | stream.rs APC fast path |
| `9b466e9c5` | terminal: fix count-limited page iteration | pending-small BL-3 | `pagelist/iter.rs` (81/17 diff, has tests; pin-era iterator) |
| `73ac36fa5` | terminal/search: discard destroyed screen state | missing BL-1 | search/thread.rs + screen.rs |
| `86e1f7b8b` | terminal/search: reject replaced result pages | missing BL-1 | — |
| `5d8eb78b7` | terminal/search: reset pins while feeding | missing BL-1 | — |
| `d239f9805` | terminal/search: drop pruned selections | missing BL-1 | — |
| `cfce1cd56` | terminal: duplicate hyperlinks before replacement | pending-small BL-3 | screen.rs cursor hyperlink vs page replacement (19/3) |
| `dace6d1c6` | terminal: handle aliased title updates | pending-small BL-3 | terminal.rs set_title self-alias (12/3) |
| `a14a619ce` | terminal: handle aliased pwd updates | pending-small BL-3 | terminal.rs set_pwd self-alias (12/3) |
| `0c299000f` | terminal: preserve aliased selection pins | pending-small BL-3 | screen.rs select() aliasing (39/2) |
| `91f0cf67d` | terminal: preserve tabstops on resize failure | **PARTIAL** | fix at terminal.rs:4362; FailingAllocator test hunk not portable (§1) |
| `fdd255c78` | terminal: reserve title terminator atomically | pending-small BL-3 | terminal.rs title NUL sentinel (23/4) |
| `1d5b6f90e` | terminal: reserve pwd terminator atomically | pending-small BL-3 | terminal.rs pwd NUL sentinel (23/4) |
| `04810273a` | terminal: wrap implicit hyperlink identifiers | pending-small BL-3 | screen.rs implicit hyperlink id counter (39/2) |
| `b287f6d1a` | terminal: handle stored grapheme boundaries | pending-small BL-3 | terminal.rs print grapheme path (23/8) |
| `e727a3658` | terminal: clear rows using stored page width | pending-small BL-3 | screen.rs clearRows (28/8) |
| `b6f34be44` | terminal: clamp mirrored selection corners | pending-small BL-3 | selection.rs (30/4) |
| `607160657` | terminal: clamp cloned selections to page width | pending-small BL-3 | screen.rs clone path (62/7) |
| `08e376f23` | terminal: bound selection drag geometry | pending-small BL-3 | selection_gesture.rs drag geometry (75/3) |
| `a55850c98` | terminal: use previous page width for cursor cells | pending-small BL-3 | screen.rs resize cursor cells (22/1) |
| `a9f5b7eba` | terminal: clamp selection rows to page width | pending-small BL-3 | selection.rs (48/4) |
| `fa8cae88b` | terminal: use destination width for line selection | pending-small BL-3 | screen.rs selectLine (32/1) |
| `3d08161b5` | terminal: handle empty tabstop ranges | **PORTED** | tabstops.rs:315 + test (§1) |
| `8f1c2fe95` | terminal: handle backwards selection timestamps | **PORTED** | selection_gesture.rs:790 + test (§1) |
| `0cb004734` | terminal: ignore empty cell clears | **PORTED** | screen.rs:2017 + test (§1) |
| `d6e24d985` | terminal: make pin traversal width-aware | pending-small BL-3 | pagelist/pin.rs traversal (120/39 — medium) |
| `30b42f42a` | terminal: reject unrepresentable pin coordinates | pending-small BL-3 | pagelist/pin.rs (49/3) |
| `e6e4a9fdc` | terminal: widen cell screen coordinates | pending-small BL-3 | pagelist coordinates (42/1) |
| `afbf5ba15` | terminal: handle minimum prompt scroll delta | pending-small BL-3 | pagelist/scroll.rs scroll_prompt (11/1) |
| `c753fe4a4` | terminal: handle minimum row scroll delta | pending-small BL-3 | pagelist/scroll.rs DeltaRow (22/6) |
| `6236d3859` | Misc runtime safety fixes (#13275) | n/a-merge | constituents above/below |
| `0aaedf436` | terminal: saturate origin cursor offsets | **PORTED** | terminal.rs:2495 + test (§1) |
| `0ff4e41b2` | terminal: fix pin wrapping at row boundaries | pending-small BL-3 | pagelist/pin.rs wrapping (58/6) |
| `5bc6588e4` | terminal/search: ignore empty search needles | pending-small BL-3 | search/{active,sliding_window,viewport}.rs (31/2; pin-era search) |
| `a23d90c89` | search: reset cached results after resize (#13274) | n/a-merge | constituent below |
| `627518447` | terminal/search: reset cached results after resize | pending-small BL-3 | search/screen.rs (73/22 — medium; pin-era search) |
| `f6f79acce` | terminal: dispatch APC string bytes in bulk slices | missing BL-4 | stream/apc/kitty + bench/synthetic surface (12 files) |
| `7e02af879` | scrollback page compression (#13264) | n/a-merge | constituents = BL-1 |
| `4f53b846b` | renderer: move State declaration to top of file | n/a | pure Zig decl reordering, no semantic content; Rust module layout differs |
| `9a4bd2120` | terminal: optimize LZ4 decoding + differential tests | missing BL-1 | compress/lz4 does not exist |
| `d34b54e9b` | renderer: hand off state mutex to avoid starving frames | missing BL-1 | renderer/state.rs + termio exec; compression-driven |
| `25e624569` | renderer: avoid starving scrollback compression | missing BL-1 | — |
| `172f15da3` | terminal: expose compression through libghostty-vt | missing BL-1 | — |
| `6d5dda40d` | inspector: clarify page compression memory | missing BL-1 | — |
| `0fb89f4ff` | terminal: configure scrollback compression | missing BL-1 | — |
| `95685afd2` | terminal: compress offscreen scrollback history | missing BL-1 | — |
| `181254d36` | terminal: debounce compression by page activity | missing BL-1 | — |
| `b9bb50c83` | terminal: optimize page compression codec | missing BL-1 | — |
| `461562ca4` | terminal: enable idle scrollback compression | missing BL-1 | — |
| `f8c217e55` | terminal: track pending scrollback compression | missing BL-1 | — |
| `0d015b2fc` | terminal: preserve compressed pages during search | missing BL-1 | — |
| `e7969ed43` | terminal: make PageList compression self-contained | missing BL-1 | — |
| `70e788f06` | terminal: add incremental scrollback compression | missing BL-1 | — |
| `9156ada16` | terminal: add cold-history compression pass | missing BL-1 | — |
| `421fe8dab` | terminal: integrate compressed pages into PageList | missing BL-1 | — |
| `f6fd4cb08` | terminal: generalize page memory reclamation | missing BL-1 | — |
| `ebc3ffd22` | terminal: add compressed page representation | missing BL-1 | — |
| `c62c15984` | terminal: add standalone LZ4 block codec | missing BL-1 | — |
| `9c9cf3e82` | renderer: avoid allocating when no active links | missing BL-4 | `crates/rustty-renderer/src/link.rs` exists; perf change (82/14) |
| `60121a039` | Revert "termio: bound POSIX read-ahead on non-Darwin" | n/a | reverts `bed47168c`; net-zero pair |
| `896aca499` | terminal: return free-listed page memory to the OS | missing BL-2 | pagelist pool decommit |
| `c442eb4b3` | terminal: document the pool reuse zeroing invariant | pending-small BL-3 | comments + asserts over pagelist/screen/page (34/6) |
| `e44f5cb0f` | terminal: guard RefCountedSet zero-capacity lookups | **PORTED** | ref_counted_set.rs:825 + style.rs test (§1) |
| `8307349ec` | terminal: fix increaseCapacity growth from zero-capacity dimensions | pending-small BL-3 | pagelist/grow.rs + screen.rs (112/15 — medium, has tests) |
| `16e4b5e98` | terminal: track page ownership explicitly on pagelist nodes | missing BL-1 | compression-prep restructure (209/72) |
| `b953bb346` | terminal: fix bitmap allocator chunk region sizing | **PORTED** | bitmap_allocator.rs:335 + 2 tests (§1) |
| `3ff6d08fa` | lib-vt: report OSC and Kitty color queries (#13239) | n/a-merge | constituents below |
| `14c829883` | terminal: report OSC color queries in lib-vt | missing BL-5 | terminal/c + osc/kitty color query surface |
| `20a1bfa5f` | fix: pass RGB color inputs by pointer | missing BL-5 | include/ghostty/vt/color.h ABI change |
| `bb0ac4c72` | termio: don't bridge pty reads while the parser is idle | missing BL-4 | rustty-termio exec |
| `bed47168c` | termio: bound POSIX read-ahead on non-Darwin | n/a | reverted upstream by `60121a039` |
| `77190bd02` | terminal: handful of scroll region optimizations | missing BL-4 | screen.rs/terminal.rs scroll paths (496/38, has tests) |
| `cabbdee32` | Fix adjust-cursor-height regression (#13225) | n/a-merge | constituents `dac341cad`+`e8f3f6c43` — PORTED |
| `446f80f4e` | terminal: render state update optimizations | missing BL-4 | render.rs + c/render + lib_vt (1348/209) |
| `b5053153f` | terminal: log unsupported-input messages once per distinct value | missing BL-4 | stream.rs logging dedup (85/10) |
| `8d663a76e` | terminal: release style refs per run in clearCells | pending-small BL-3 | screen.rs clear_cells style-ref batching (15/3) |
| `cb2d78587` | terminal: fill style-only cell runs in bulk in printSliceFill | missing BL-4 | terminal.rs print slice fill (BL-4 fast-path family) |
| `300f42c7a` | terminal: handle CSI entry bytes inline in consumeUntilGround | missing BL-4 | stream.rs |
| `083d9709b` | terminal: decode ASCII inline in SIMD scan for ESC | missing BL-4 | rustty-simd (src/simd/vt.cpp analogue) |
| `e8f3f6c43` | font/sprite: regression test for cursor-height metric | **PORTED** | face.rs test (§1) |
| `dac341cad` | font/sprite: cursor height respects adjust-cursor-height | **PORTED** | face.rs:704-716 (§1) |
| `cb4c49fbf` | terminal: scalar UTF-8 decode consumes partial sequences cut by ESC | missing BL-4 | rustty-simd vt scalar path (90/4, has tests) |
| `4c5b1d5d5` | bench: terminal-stream reads 64KiB chunks | pending-small BL-3 | rustty-bench terminal_stream (5/2) |
| `cee35cabf` | terminal: skip style map update when SGR unchanged | **PORTED** | screen.rs:2752 (§1) |
| `253e4f9c3` | terminal: bulk-parse CSI parameter bytes at slice level | missing BL-4 | stream.rs (76/0, has tests) |
| `1a88f3622` | terminal: dispatch CSI finals directly from stream fast paths | missing BL-4 | stream.rs (89/0) |
| `47e26df60` | terminal: batch printed codepoint runs into direct row fills | missing BL-4 | stream/terminal/termio print pipeline (726/3) |
| `258de36d1` | benchmark: terminal-stream uses full terminal handler | missing BL-4 | rustty-bench |
| `2f0e6659d` | termio: pipeline pty reads to overlap parsing with draining | missing BL-4 | rustty-termio exec (342/57) |
| `63e75e86c` | lib-vt: many more color utility APIs (#13206) | n/a-merge | constituent below |
| `2970e9a2a` | lib-vt: many more color utility APIs | missing BL-5 | color.h/sgr.h + c/color + x11_color surface (1189/131) |
| `4a7cabc4f` | lib-vt: color scheme report encoder (#13192) | n/a-merge | constituent below |
| `715ef6c15` | fix: max window clamp to monitor size (#13171) | n/a-merge | constituent `004c88e41` (gtk) |
| `f00e90694` | lib-vt: add color scheme report encoder | missing BL-5 | include/ghostty/vt/color_scheme.h + c/color_scheme |
| `004c88e41` | fix: set max window clamp to current monitor size | n/a-tier BL-8 | src/apprt/gtk — tier unported |
| `65e61282a` | lib-vt: add unicode grapheme width API | missing BL-5 | c/unicode + unicode/grapheme surface |
| `688750903` | Kitty dnd parser (#13029) | n/a-merge | constituents = BL-7 |
| `cca51729a` | lib-vt: scroll-to-row viewport scrolling (#13179) | n/a-merge | constituent below |
| `3a2e28329` | lib-vt: add scroll-to-row viewport scrolling | missing BL-5 | PageList `Scroll::Row` core pre-exists at pin (scroll.rs:91); missing piece is the `ScrollViewport` `Row` tag (terminal.rs:318 has Top/Bottom/Delta only) + C tag/docs |
| `fc5a72772` | lib-vt: add unicode codepoint width API | missing BL-5 | c/unicode |
| `0a410f18e` | terminal: clipboard_set effect for OSC 52 writes | missing BL-5 | `grep -r clipboard_set crates/rustty-vt*` → 0 |
| `8fe5913f7` | kitty/gfx: fix build without 64-bit atomics | missing BL-6 | fix over `bdc0b6c19`'s generation stamps |
| `bdc0b6c19` | kitty/gfx: generation stamps, delete transmit time | missing BL-6 | `transmit_time` still present in rustty kitty (8 hits) |
| `f245cdc66` | lib-vt: expose selection gesture to Zig | missing BL-5 | 1-line lib_vt.zig export |
| `aea63d71f` | libghostty: fix utf-8 grapheme length overflow | pending-small BL-3 | rustty-vt-c render seam (79/20, has tests) |
| `e5b65a2ce` | config: gtk-horizontal-tab-scroll option (#12659) | n/a-merge | constituents = BL-8 |
| `0a117e079` | gtk: fix missing dbus connection causing crash | n/a-tier BL-8 | gtk tier unported |
| `5fa37f2b7` | correct type for remaining rows in PageList.scroll | **PORTED** | pagelist/scroll.rs:135,166 (§1) |
| `c488ccda6` | terminal: correct OSC 72 doc comments | missing BL-7 | kitty_dnd parser absent |
| `a6132c18b` | terminal: OSC 72 EventType/Option doc comments | missing BL-7 | — |
| `dd6c09bcf` | terminal: clean up OSC 72 parser comments, add tests | missing BL-7 | — |
| `91c87e2cf` | terminal: parse kitty drag and drop protocol (OSC 72) | missing BL-7 | `grep -r kitty_dnd crates/rustty-vt/src` → 0 |
| `b5f5ca078` | Add "available since" | n/a-tier BL-8 | Config docs for gtk option below |
| `5ea45919d` | config: add gtk-horizontal-tab-scroll option | n/a-tier BL-8 | gtk tier unported; Config option lands with that tier |

---

## 4. Standing ritual (R12)

Re-run this triage per sub-project completion and at minimum whenever the pin conversation resumes:
1. `git -C /Users/gael/projects/ghostty fetch origin` (fetch ONLY — checkout stays read-only, R3).
2. `git log --oneline <last-triaged-head>..origin/main -- src/ include/`; append new rows here.
3. Verdict each commit with file:line evidence; port small clear fixes WITH their test hunks; structural work gets/extends a BL-n entry.
4. Last triaged head: **`53bd14fec`** (2026-07-10, this pass).
