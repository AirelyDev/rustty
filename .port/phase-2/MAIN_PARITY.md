# MAIN_PARITY — upstream ghostty parity ledger (R12, T19)

**Pin (oracle reference):** `4749c4e93` (2026-06-20, "i18n(kk): translate missing Close Split string (#13052)") — ancestor of `origin/main`.
**Upstream head at triage:** `origin/main` = `53bd14fec` (fetched 2026-07-10; `git fetch` only — checkout untouched, R3).
**Scope:** `git -C /Users/gael/projects/ghostty log --oneline 4749c4e9..origin/main -- src/ include/` → **129 commits** (116 non-merge + 13 PR-merge commits).

**Doctrine (verbatim, binding):**
> a merge does NOT port the fix; only an explicit .rs change does

> ALSO port the test hunks from the same commit

Updating/fetching the reference checkout ports nothing. Every verdict below is against the rustty working tree as of this triage.

> **SCOPE WARNING (`P3-W10 T2`, 2026-07-31).** This ledger was triaged at `53bd14fec` (2026-07-10) and
> scoped to `src/` + `include/` only. It therefore **cannot speak** for the `53bd14fec..6ad1fe7d8`
> range, nor for `pkg/`, `po/`, or `macos/` in any range. It is a Phase-2 artifact kept for provenance;
> the authoritative Phase-3 record is `docs/superpowers/plans/2026-07-29-phase3-upstream-resync.md`
> plus `.port/phase-3/`. **Eighteen** rows below carried `missing BL-1` verdicts that were wrong — the
> work landed in WS6/WS7 (and `P3-W5 T3`) — and were corrected by `P3-W10 T2`, each with a
> `file:line` **and** a named guard test, per `.port/phase-3/README.md` ("a verdict of 'already
> ported' is only acceptable with evidence"). Thirteen were the set the plan enumerated; the other
> five (`0bad91adb`, `dd956f37c`, `91a63e528`, `a804e3ac7`, `f1a5fab45`) are the same BL-1
> generation series, were found equally ported while checking the first thirteen, and meet the same
> evidence bar.
>
> **RESOLVED 2026-07-31 — the ~15 stale BL-1 rows below have been re-triaged.** All fourteen now carry
> `**PORTED**` with `file:line` evidence and a named guard test, to the same bar `P3-W10 T2` used. Twelve
> were confirmed by per-commit provenance citations in the Rust tree; the two that carried residual
> partial claims (`181254d36`'s `PageList.zig` hunk, `461562ca4`'s `Config`/inspector/benchmark hunks)
> were checked hunk-by-hunk against the oracle's `--stat` and found ported as well. The specific claim
> "compress/lz4 does not exist" was false when written: `crates/rustty-vt/src/compress/lz4.rs` is 998
> lines with 13 tests, and **G-LZ4** holds its byte stream identical to the oracle's on 7,976
> differential points (1,631 of which diverge under a naive port, so that gate is load-bearing).
>
> Note what this cost: `9156ada16` read `missing BL-1` while **21 of its tests were genuinely absent**
> and nobody noticed, because a row already marked missing invites no scrutiny. A wrong "missing" is
> not the safe direction of error — it is camouflage. The 19 that were found came from `G-TESTS`
> reading the live oracle, not from this ledger.
>
> ~~**STILL STALE, NOT CLOSED BY `P3-W10` (do not read these as absent).** ~15 further rows below —
> the core BL-1 compression series (`c62c15984`, `ebc3ffd22`, `f6fd4cb08`, `421fe8dab`, `9156ada16`,
> `70e788f06`, `e7969ed43`, `f8c217e55`, `b9bb50c83`, `9a4bd2120`, `95685afd2`, `461562ca4`,
> `181254d36`, `16e4b5e98`, …) still say `missing BL-1`, and at least one says "compress/lz4 does
> not exist" when `crates/rustty-vt/src/compress/lz4.rs` demonstrably does and gate **G-LZ4** proves
> its byte stream identical to the oracle's on 7,976 differential points. `P3-W10 T2` was scoped to
> the rows the plan enumerated and did **not** re-triage these; they need per-commit evidence before
> they may be struck. **Owner: next resync.** Treat their verdicts as unreliable in the meantime.~~
>
> *(Struck 2026-07-31 — superseded by the RESOLVED note above. Kept, not deleted, because it is the
> record of what was believed and for how long.)*

**Verdict legend:**
- `PORTED` — explicit .rs change landed this pass (T19), with the commit's test hunks.
- `PARTIAL` — fix ported; some hunk (usually a test) not portable, reason recorded.
- `missing` — not in rustty; `BL-n` names the backlog entry that owns it.
- `pending-small` — missing AND assessed portable as a small/medium standalone fix (BL-3 queue for the next parity pass).
- `n/a-merge` — PR merge commit; constituents triaged individually.
- `n/a` — no port action possible/meaningful (reverted upstream, or pure Zig decl reordering).
- `n/a-tier` — touches a tier not yet ported (GTK apprt); owned by that tier's port plan (BL-8).

---

## 1. Ported this pass (T19) — 10 commits

> The `91f0cf67d` PARTIAL row that lived here was CLOSED by WS8 T17 and moved to
> §3 as a full strike (the `Tabstops::resize` tripwire is now `pub(crate)`, and
> `terminal_resize_preserves_tabstops_on_allocation_failure` in `terminal.rs`
> arms it on exactly the `Terminal::resize` -> `Tabstops::init` path upstream's
> `FailingAllocator` test exercises).

| upstream | subject | rustty change (evidence) | test hunks |
|---|---|---|---|
| `b953bb346` | terminal: fix bitmap allocator chunk region sizing | `crates/rustty-vt/src/bitmap_allocator.rs:335` (`aligned_chunk_count * CHUNK_SIZE`) | both: `BitmapAllocator_layout_chunks_region_matches_bitmap_addressable_bytes`, `BitmapAllocator_layout_small_capacity_cannot_alloc_out_of_bounds` |
| `e44f5cb0f` | terminal: guard RefCountedSet lookups against zero-capacity sets | `crates/rustty-vt/src/ref_counted_set.rs:825` (`table_cap == 0` guard) | `style.rs` `set_zero_capacity` (0xFF adjacent-memory probe) |
| `3d08161b5` | terminal: handle empty tabstop ranges | `crates/rustty-vt/src/tabstops.rs:315` (`interval == 0 \|\| cols <= 1` return) | `tabstops_interval_with_zero_columns` |
| `0aaedf436` | terminal: saturate origin cursor offsets | `crates/rustty-vt/src/terminal.rs:2495` (`saturating_add` on both origin offsets) | `terminal_set_cursor_pos_saturates_overflowing_origin_offsets` |
| `0cb004734` | terminal: ignore empty cell clears | `crates/rustty-vt/src/screen.rs:2017` (`cells.len() == 0` guard before integrity pause) | `screen_clear_cells_empty_range` |
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
- **BL-2 — `896aca499` return free-listed page memory to the OS** — **PORTED (WS8 T12/T13).** `decommit_pool_item`/`recommit_pool_item` in `crates/rustty-vt/src/pagelist/mod.rs` with hand-rolled `extern "C" madvise` + per-OS constants in the style of `page.rs`'s `mmap`/`munmap` (no new `libc` dependency); wired into `destroy_node_ext` and both pooled `pool.pages.create()` sites. The `madvise` arms are outside Miri's memory model and are registered in `MIRI_EXCEPTIONS.md` "Category 2" (the `#[cfg(test)]` memset arm, which upholds the pool zero-invariant, IS still Miri-run). Empirical verification is `crates/rustty-vt/examples/rss_probe.rs` — macOS MEASURED 9.0 MiB returned on scrollback clear vs 0.0 MiB with the `madvise` call disabled. Linux (`MADV_DONTNEED`) arm not yet run.
- **BL-3 — pending-small queue** — still an OPEN bucket. The 12 commits struck in §3 below landed in WS8 (T5-T11): `cfce1cd56`, `04810273a`, `1d5b6f90e`, `a14a619ce`, `fdd255c78`, `dace6d1c6`, `0c299000f`, `b6f34be44`, `a9f5b7eba`, `607160657`, `fa8cae88b`, `08e376f23`. The remaining rows still marked `pending-small BL-3` are the follow-up pass. Each is a small/medium single-concern fix to pin-era ported code; port assertion-for-assertion WITH its test hunks, one commit per upstream commit.
- **BL-4 — stream/terminal/termio performance series** (#13206-era + later: SIMD ESC/ASCII inline, CSI bulk parse/dispatch, print run batching, scroll-region opts, render-state lock-hold opts, APC bulk dispatch, pty read pipelining, per-run style release, once-per-value logging). Perf-intent work over ported code; several commits interlock (stream fast paths ↔ terminal handlers ↔ bench harness). Port as a series with the bench corpus commits (`4c5b1d5d5`, `258de36d1`).
- **BL-5 — lib-vt C-API expansion** — **CLOSED by WS5** (color utility APIs, color-scheme report encoder, OSC/Kitty color-query reporting, unicode width APIs, scroll-to-row tag, protocol-neutral clipboard_write/OSC-52 effect, RGB-by-pointer ABI fix, selection-gesture export). All 10 BL-5 oracle commits ported (WS5 T2-T18); the 13 new `rustty_*` exports + `clipboard_write` option 26 are live and header-gated. `ScrollViewport` now has a `Row` variant (`crates/rustty-vt/src/terminal.rs`); `clipboard` is now a live rustty-vt module (the protocol-neutral `clipboard_write` effect supersedes the intermediate `clipboard_set` form).
- **BL-6 — kitty/gfx generation stamps** — **PORTED (WS8 T14/T15/T16).** Process-global `AtomicU64` counter + `next_generation()`; `ImageStorage::generation`/`mark_mutated`; `Image::generation` replaces `transmit_time`; newest-by-number lookup and eviction ordering key on the stamp; delete only marks a mutation when it removed something. Renderer texture-cache staleness keys on the generation. C API adds `RUSTTY_KITTY_GRAPHICS_DATA_GENERATION = 2` and `RUSTTY_KITTY_IMAGE_DATA_GENERATION = 9`. Evidence: `grep -rn transmit_time crates/` → 0 code hits (one PORT NOTE comment). `8fe5913f7` is N/A — every rustty target reports `target_has_atomic = "64"`.
- **BL-7 — OSC 72 kitty drag-and-drop parser** — **PORTED (WS8 T2-T4).** New `crates/rustty-vt/src/osc/parsers/kitty_dnd_protocol.rs` + `Command::KittyDndProtocol`/`State::S72` wiring and the `stream.rs` unimplemented-OSC dispatch arm (parse-only, matching upstream scope). Evidence: `grep -r kitty_dnd crates/rustty-vt/src` → non-zero hits.
- **BL-8 — GTK tier** (`004c88e41`, `0a117e079`, `5ea45919d`'s **behaviour half only**). GTK apprt is not yet ported (heavy-tier plan: GTK last); these belong to that tier's pre-port commit sweep. **Correction (P3-W9 T3):** the `gtk-horizontal-tab-scroll` **Config option** and its docs (`b5f5ca078`) were wrongly listed here — a `Config` field has no GTK-apprt dependency, and deferring it made the 204-field contract silently diverge from the oracle. Both landed in `P3-W9 T3`; only `5ea45919d`'s `src/apprt/gtk/class/surface.zig` scroll handling remains, as `P3-W8 T5 step 3`.

---

## 3. Per-commit verdict table (all 129, newest → oldest)

| upstream | subject | verdict | mapping / evidence |
|---|---|---|---|
| `53bd14fec` | terminal: page serial as generation marker (#13282) | n/a-merge | constituents below |
| `89eca063e` | terminal/search: avoid pruning history on reload | **PORTED** (WS7; row corrected `P3-W10 T2`) | `crates/rustty-vt/src/search/screen.rs:601` `history_result_is_valid`, `:618` `validate_selected_history`, called from `:728` (reload defers pruning) and `:841`. **Guard:** `search::screen::tests::reload_defers_pruning_unselected_history_results` (`search/screen.rs:2973`). |
| `0bad91adb` | terminal: renew generations for direct row shifts | **PORTED** (WS7 + `P3-W5 T3`; row corrected `P3-W10 T2`) | `crates/rustty-vt/src/pagelist/mod.rs` `invalidate_node_layout`; callers `screen.rs:1693,1777,1829` (`cursor_scroll_above_rotate` at `:1754`) and `terminal.rs:4006` `invalidate_full_width_row_range` (`:4134,4376`). **Guard:** `screen::tests::screen_cursor_scroll_region_up_renews_page_generation`. (The 4th `invalidateNodeLayout` site is separately evidenced on the `77190bd02` row below.) |
| `d4ac93a03` | clipboard_set effect for OSC 52 (#13182) | n/a-merge | constituents below |
| `634ef7198` | terminal: make clipboard writes protocol neutral | **PORTED** | clipboard.rs (Location/Content/Write/WriteResult) + clipboard_write effect + vt-c trampoline/Option 26/types/terminal.h/example (WS5 T16-T17) |
| `f6d9a582e` | terminal: rename page serial floor as epoch | **PORTED** (WS7; row corrected `P3-W10 T2`) | `crates/rustty-vt/src/pagelist/mod.rs:1063` `page_serial_epoch` field, `:1801` integrity check ("page serial predates epoch"), `:1944` `reset` begins a new epoch, `:2557` `node_is_valid` epoch rejection; consumer at `search/screen.rs:672`. **Guard:** `pagelist::tests` epoch assertions at `pagelist/tests.rs:1544-1580` (reset advances the epoch; a list mutation does not) and `:2120`. |
| `dd956f37c` | terminal: renew generations for shifted rows | **PORTED** (WS7; row corrected `P3-W10 T2`) | tests-only upstream, over the `invalidate_node_layout` erase paths (`crates/rustty-vt/src/pagelist/erase.rs`). **Guards:** `pagelist::tests::pagelist_row_erasure_renews_affected_page_generations` and `…::pagelist_trailing_row_truncation_renews_page_generation` — both assert `!node_is_valid(node, old_serial)` after the mutation. |
| `d0a26d146` | terminal: reschedule compression after page replacement | **PORTED** (WS7; row corrected `P3-W10 T2`) | `crates/rustty-vt/src/pagelist/grow.rs:682` (`increase_capacity`) and `:772` (`compact`) both `self.page_compression.mark_activity()` after the splice+destroy. **Guard:** `pagelist::tests::pagelist_replacements_preserve_compression_continuation_and_mark_activity` (`pagelist/tests.rs:2521`) — asserts `activity_serial` advances by 1 while `last_serial`/`next_serial` are preserved. |
| `a89c6133c` | terminal/search: validate history before reload | **PORTED** (WS7; row corrected `P3-W10 T2`) | `crates/rustty-vt/src/search/screen.rs:601` `history_result_is_valid` + `:639` selected-history validation on the reload path. **Guard:** `search::screen::tests::reload_after_partial_history_page_erase_drops_shifted_selection_first` (`search/screen.rs:3045`). |
| `c9ad708ef` | terminal: validate flattened highlight generations | **PORTED** (WS7; row corrected `P3-W10 T2`) | `crates/rustty-vt/src/render.rs:224` per-row `serial` field, `:868-884` row-serial writes keyed on `node.serial`, `:1113-1123` chunk-vs-row serial match before a flattened highlight is accepted. **Guard:** `render::tests::flattened_highlights_require_matching_page_serial` (`render.rs:2388`) — rejects on `live_serial ^ 1`, accepts once the copied serial matches. |
| `c243d89bd` | terminal/search: generations in viewport fingerprints | **PORTED** (WS7; row corrected `P3-W10 T2`) | `crates/rustty-vt/src/search/viewport.rs:356` `Entry.serial`, `:434` fingerprint captures `node.serial`, `:444` comparison rejects on a serial change. **Guard:** `search::viewport::tests::fingerprint_detects_node_generation_changes` (`search/viewport.rs:596`). |
| `83aada205` | terminal/search: preserve serial order in reverse matches | **PORTED** (WS7; row corrected `P3-W10 T2`) | `crates/rustty-vt/src/search/sliding_window.rs:802` `self.chunk_buf.reverse()` — rustty's `Vec<Chunk>` is AoS, so the `serial` column is reversed with `node`/`start`/`end` by construction (upstream needed a separate `std.mem.reverse` because its `chunk_buf` is a Zig SoA `MultiArrayList`). **Guard:** the ported assertions in `search::sliding_window::tests::sliding_window_two_pages_match_across_boundary_reversed` (`sliding_window.rs:2405-2411`) pair each chunk's `node` with its OWN `serial`. |
| `91a63e528` | terminal: invalidate split source page refs | **PORTED** (WS7; row corrected `P3-W10 T2`) | `crates/rustty-vt/src/pagelist/` split path + `node_is_valid` (`pagelist/mod.rs:2554`). **Guards:** `pagelist::tests::pagelist_bounded_pruning_after_split_invalidation_preserves_live_serials` and `search::screen::tests::select_after_history_page_split_ignores_moved_results`. |
| `a804e3ac7` | terminal: invalidate partially erased page refs | **PORTED** (WS7; row corrected `P3-W10 T2`) | `crates/rustty-vt/src/pagelist/erase.rs` `invalidate_node_layout` on the partial-erase path. **Guards:** `pagelist::tests::pagelist_bounded_pruning_after_partial_erase_preserves_live_serials`, `…::pagelist_partial_erase_restarts_compression_before_continuation`, and `search::screen::tests::select_after_partial_history_page_erase_ignores_shifted_results`. |
| `f1a5fab45` | terminal: keep page serial floor conservative | **PORTED** (WS7; row corrected `P3-W10 T2`) | the serial-floor/epoch conservatism in `crates/rustty-vt/src/pagelist/mod.rs:2557` (`node_is_valid` epoch rejection) and `:1944`. **Guards:** `pagelist::tests::pagelist_repeated_bounded_pruning_after_split_preserves_live_serials`, `…::pagelist_bounded_pruning_after_front_replacement_preserves_live_serials`, `…::pagelist_bounded_pruning_after_middle_replacement_preserves_live_serials`. |
| `8c523ed03` | terminal: vectorize APC payload scanning | **PORTED** (WS10 T17, `f96957c`) | `Stream::consume_apc_string` vector `invalid` scan `stream.rs:1045`; guarded by `stream_apc_vector_invalid_branch_matches_scalar_path` `stream.rs:4406`. Row was stale — re-vetted `P3-W5`. |
| `9b466e9c5` | terminal: fix count-limited page iteration | **PORTED** (P3-W1) | `pagelist/iter.rs` forward + reverse `Limit::Count` arms + 2 tests (T2) |
| `73ac36fa5` | terminal/search: discard destroyed screen state | **PORTED** (WS7; row corrected `P3-W10 T2`) | `crates/rustty-vt/src/search/thread.rs:1212` `screen_is_valid`, called at `:1183` and `:1363`; `search/screen.rs:316` `deinit_screen_invalid` → `:338` `deinit_list_invalid` (`search/pagelist.rs:150`). **Guard:** `search::thread::tests::select_after_active_screen_removal` (`search/thread.rs:2432`). |
| `86e1f7b8b` | terminal/search: reject replaced result pages | **PORTED** (WS7; row corrected `P3-W10 T2`) | `crates/rustty-vt/src/pagelist/mod.rs:2554` `PageList::node_is_valid(node, serial)`; consumed by the history-result validation in `search/screen.rs:601`. **Guard:** `search::screen::tests::select_after_history_compaction_ignores_replaced_results` (`search/screen.rs:2915`). |
| `5d8eb78b7` | terminal/search: reset pins while feeding | **PORTED** (WS7; row corrected `P3-W10 T2`) | `crates/rustty-vt/src/search/pagelist.rs:210-219` — `feed` resets `pin.y`/`pin.x` to the new node's bottom-right cell and clears `garbage`. **Guard:** `search::pagelist::tests::feed_keeps_its_tracked_pin_within_a_shorter_page` (`search/pagelist.rs:872`) — asserts `pages.pin_is_valid(*search.pin)` after feeding across a shorter split page. |
| `d239f9805` | terminal/search: drop pruned selections | **PORTED** (WS7; row corrected `P3-W10 T2`) | `crates/rustty-vt/src/search/screen.rs:706-707` — `first_pruned = active_results.len() + i`; a selection with `idx >= first_pruned` is deinit'd and cleared. **Guard:** `search::screen::tests::select_after_partial_history_erase_drops_a_pruned_selection` (`search/screen.rs:2859`). |
| `cfce1cd56` | terminal: duplicate hyperlinks before replacement | ported (WS8 T6) | R1 fidelity reorder; the upstream use-after-free was already unreachable in rustty (owned `Vec<u8>` dupe in `start_hyperlink`) — regression test ported, no live bug fixed |
| `dace6d1c6` | terminal: handle aliased title updates | ported (WS8 T7) | terminal.rs alias-safe memmove |
| `a14a619ce` | terminal: handle aliased pwd updates | ported (WS8 T7) | terminal.rs alias-safe memmove |
| `0c299000f` | terminal: preserve aliased selection pins | ported (WS8 T8) | screen.rs select() |
| `91f0cf67d` | terminal: preserve tabstops on resize failure | ported (WS8 T17) | PARTIAL CLOSED: `ResizeFp`/`RESIZE_TW` made `pub(crate)` + `resize_tw_error_always`/`resize_tw_end` accessors; `terminal_resize_preserves_tabstops_on_allocation_failure` arms the fail point on the `Terminal::resize` -> `Tabstops::init` path |
| `fdd255c78` | terminal: reserve title terminator atomically | ported (WS8 T7) | terminal.rs title NUL sentinel |
| `1d5b6f90e` | terminal: reserve pwd terminator atomically | ported (WS8 T7) | terminal.rs pwd NUL sentinel |
| `04810273a` | terminal: wrap implicit hyperlink identifiers | ported (WS8 T6) | screen.rs implicit hyperlink id wrap |
| `b287f6d1a` | terminal: handle stored grapheme boundaries | **PORTED** (P3-W1) | debug_assert!(!brk) deleted, `grapheme_break` call kept; ported test + an end-to-end `Stream` drive; the `erase_display` workaround in the print-slice differential is removed as upstream does (T4) |
| `e727a3658` | terminal: clear rows using stored page width | **PORTED** (P3-W1) | `clear_rows` takes cells via `page.get_cells(row)`; `clear_cells` compares against `page.size.cols` (3 sites) + 2 tests (T4) |
| `b6f34be44` | terminal: clamp mirrored selection corners | ported (WS8 T9) | selection.rs top_left/bottom_right mirrored arms |
| `607160657` | terminal: clamp cloned selections to page width | ported (WS8 T10) | screen.rs clone fallback pins |
| `08e376f23` | terminal: bound selection drag geometry | ported (WS8 T11) | pt.2 fixed (empty-geometry reject + saturating pixel span); pt.1 (`pixelFromFloat`) is a VERIFIED NON-ISSUE in Rust — `f64 as u32` is saturating and NaN-safe by RFC 2884 — ported as a regression test only |
| `a55850c98` | terminal: use previous page width for cursor cells | **PORTED** (P3-W1) | `cursor_cell_end_of_prev` uses `page_pin.node.cols() - 1` + test (T4) |
| `a9f5b7eba` | terminal: clamp selection rows to page width | ported (WS8 T9) | selection.rs contained_row_cached |
| `fa8cae88b` | terminal: use destination width for line selection | ported (WS8 T10) | screen.rs select_line end pin |
| `3d08161b5` | terminal: handle empty tabstop ranges | **PORTED** | tabstops.rs:315 + test (§1) |
| `8f1c2fe95` | terminal: handle backwards selection timestamps | **PORTED** | selection_gesture.rs:790 + test (§1) |
| `0cb004734` | terminal: ignore empty cell clears | **PORTED** | screen.rs:2017 + test (§1) |
| `d6e24d985` | terminal: make pin traversal width-aware | **PORTED** (P3-W1) | end state of the `0ff4e41b2` -> `d6e24d985` pair; `pagelist/pin.rs` pin/left_wrap/right_wrap/up_overflow/down_overflow + 7 tests (T1) |
| `30b42f42a` | terminal: reject unrepresentable pin coordinates | **PORTED** (P3-W1) | `pagelist/pin.rs` `point_from_pin` checked adds + test (T3) |
| `e6e4a9fdc` | terminal: widen cell screen coordinates | **PORTED** (P3-W1) | `pagelist/access.rs` `screen_point` u32 accumulator + test (T3) |
| `afbf5ba15` | terminal: handle minimum prompt scroll delta | **PORTED** (P3-W1) | `pagelist/scroll.rs` `scroll_prompt` `delta.unsigned_abs()` + test (T3) |
| `c753fe4a4` | terminal: handle minimum row scroll delta | **PORTED** (P3-W1) | `pagelist/scroll.rs` `amount = n.unsigned_abs()` + test (T3) |
| `6236d3859` | Misc runtime safety fixes (#13275) | n/a-merge | constituents above/below |
| `0aaedf436` | terminal: saturate origin cursor offsets | **PORTED** | terminal.rs:2495 + test (§1) |
| `0ff4e41b2` | terminal: fix pin wrapping at row boundaries | **PORTED** (P3-W1) | superseded by `d6e24d985` in the same functions; its 4 regression tests are ported and pass against that end state (T1) |
| `5bc6588e4` | terminal/search: ignore empty search needles | **PORTED** (P3-W1) | empty-needle early returns in `SlidingWindow::next` and `ActiveSearch::update`; saturating `overlap_len` in `search/viewport.rs` (the oracle does NOT saturate `search/active.rs:132` — its early return covers it, so the WS7 Phase C M2 note over-specified by one site); the `sliding_window.rs` PORT NOTE claiming Zig returns null for an empty needle was the bug, not the code (REACHABILITY row 8) (T5) |
| `a23d90c89` | search: reset cached results after resize (#13274) | n/a-merge | constituent below |
| `627518447` | terminal/search: reset cached results after resize | **PORTED** (P3-W1) | `reset_if_dimensions_changed` extracted; called from `feed`, first in `reload_active` (early return), first in `select` + 2 tests (T5) |
| `f6f79acce` | terminal: dispatch APC string bytes in bulk slices | **PORTED** (WS10 T17, `f96957c`; termio arm `P3-W7 T6`, `8da32f4`) | `Action::ApcPutSlice` + `apc_put_slice` `stream.rs:605`, `consume_apc_string:1045`, `apc::Handler::feed_slice`, `crates/rustty-bench/src/apc_parser.rs`. Row was stale — re-vetted `P3-W5`. |
| `7e02af879` | scrollback page compression (#13264) | n/a-merge | constituents = BL-1 |
| `4f53b846b` | renderer: move State declaration to top of file | n/a | pure Zig decl reordering, no semantic content; Rust module layout differs |
| `9a4bd2120` | terminal: optimize LZ4 decoding + differential tests | **PORTED** (re-triaged 2026-07-31) | cited by 2 Rust files. `crates/rustty-vt/src/compress/lz4_differential.rs` (633 lines) IS the differential harness this row said did not exist; the claim "compress/lz4 does not exist" was false when written and is false now. |
| `d34b54e9b` | renderer: hand off state mutex to avoid starving frames | **PORTED** (WS7 D4) | renderer/state.rs `lock_demand`/`unlock_demand`/`yield_to_demand` + `HandoffPark` (futex substitute) + 4 tests; generic.rs AND generic/linux_impl.rs `update_frame` critical sections; termio/exec.rs `threadMainPosix` batch boundary. No compression dependency. |
| `25e624569` | renderer: avoid starving scrollback compression | **PORTED** (WS7 D3) | Thread.zig-only commit (8/1). The contended-wake branch is in `renderer_seam.rs` `Compression::wake`: `else if state_of(&self.completion) == Active { return; }` plus the amended "…but is necessary when no timer is already active" comment. Landed as part of D3's final-shape `Compression` port. |
| `172f15da3` | terminal: expose compression through libghostty-vt | **PORTED** (WS7 D5) | vt-c `terminal.rs` `CompressionMode`/`CompressionResult` aliases + `compression_activity`/`compress` + `lib.rs` `rustty_terminal_compression_activity`/`rustty_terminal_compress` exports + 2 tests; `include/rustty/vt/terminal.h` enums/decls/`## Scrollback Compression` section, `include/rustty/vt.h` example refs, cbindgen `after_includes` enum pins, `example/c-vt-compression`. The `Terminal.zig` half landed as WS7 D1 (`782e574`); the `PageList.compress` full-mode `unsupported` hunk already landed in WS7 Phase A/B (`pagelist/compress.rs:501`). |
| `6d5dda40d` | inspector: clarify page compression memory | **PORTED** (WS6/WS7; row corrected `P3-W10 T2`) | `crates/rustty-inspector/src/widgets/pagelist.rs:198` the "Page Compression" section header, `:203`/`:518` `compressionTable(pages)`, `:473` the reworded "Maximum uncompressed logical page memory…" help text, `:718` `compressionTextRow`. **Guards:** `widgets::pagelist::ws7_tests::pagelist_widget_reports_compression_memory` (`:2604`) and `…::pagelist_compression_view_renders_live_numbers` (`:2634`), plus `…::format_bytes_matches_oracle_units` (`:2822`) for the unit formatting the commit relies on. |
| `0fb89f4ff` | terminal: configure scrollback compression | **PORTED** (WS7 D2 + D3) | Both hunks are in. `Config.zig` hunk = WS7 D2 (`config_struct.rs` `scrollback_compression: bool` default `true`, + the 50 MB `scrollback-limit` docs). **VERIFIED `P3-W3` T6 finale, 2026-07-30:** the `P3-W3` plan's finale bullet claimed "the 50 MB default is **not** from `0fb89f4ff`" and asked for this parenthetical to be corrected. It is the PLAN that is wrong: `git show 0fb89f4ff -- src/config/Config.zig` contains `-@"scrollback-limit": usize = 10_000_000, // 10MB` / `+@"scrollback-limit": usize = 50_000_000, // 50MB` plus the surrounding "with scrollback compression enabled" doc rewrite, and the commit message says "Raise the default logical scrollback limit from 10 MB to 50 MB". Row left as written. (`P3-W3` T4 later renames the option to `scrollback-limit-bytes`, keeping the 50 MB default.) `renderer/Thread.zig` hunk = WS7 D3: `rustty-renderer` `thread::DerivedConfig.scrollback_compression` + the `RendererConfig::scrollback_compression` accessor (impl'd in rustty-config's `renderer_config.rs` — the acyclic accessor-trait pattern), and in core's `renderer_seam.rs` the `Compression` struct (ported at its FINAL oracle@main shape) + its `Thread` field, `init`/`deinit` gates, the `changeConfig` disabled->enabled activity reset, and the `wakeupCallback` `compression.wake(t)` site. 4 tests (1 renderer DerivedConfig both-arms, 3 core end-to-end: a real `xev.Loop` + real `Terminal` idle pass that compresses pages, the disabled-option inert arm, and the `changeConfig` transition). NOTE: the renderer `Thread` lives in `rustty-core/src/renderer_seam.rs`, not `rustty-renderer` (it names apprt + config types) — the WS7 plan's file list was wrong about this. |
| `95685afd2` | terminal: compress offscreen scrollback history | **PORTED** (re-triaged 2026-07-31) | cited by 1 Rust file. Guard: `pagelist_compress_full_leaves_viewport_and_active_resident`. |
| `181254d36` | terminal: debounce compression by page activity | **PORTED** (re-triaged 2026-07-31) | `renderer/Thread.zig` hunk (the 253-line `Compression` rewrite, FINAL shape) by WS7 D3 (`renderer_seam.rs`); `Terminal.zig`/`main.zig` by WS7 D1 (`terminal.rs:3827`). The `PageList.zig` hunk this row still called missing **is also ported**: `pagelist/compress.rs:132` `activity_serial` (the oracle's `u48` modeled by `ACTIVITY_SERIAL_MASK` at `:159`, wrapping add at `:177`), `:175` `mark_activity`, and the reset-preserves-activity shape at `:183`. Guard: `pagelist_replacements_preserve_compression_continuation_and_mark_activity` — asserts `activity_serial` 42->43 while `last_serial`/`next_serial` are preserved, so it fails if `mark_activity()` is dropped OR replaced by a `reset()`. |
| `b9bb50c83` | terminal: optimize page compression codec | **PORTED** (re-triaged 2026-07-31) | cited by 1 Rust file; codec optimization folded into `compress/lz4.rs`, held byte-exact by **G-LZ4**. |
| `461562ca4` | terminal: enable idle scrollback compression | **PORTED** (re-triaged 2026-07-31) | `renderer/Thread.zig` (+157, the original `Compression` scheduler) by WS7 D3 at the later FINAL shape — `181254d36` rewrote it wholesale, so porting the intermediate shape would have been strictly wrong. The three hunks this row still called missing are **all ported**: `Config.zig` -> `config_struct.rs:495` (`scrollback-compression` key) + `:2343`; `inspector/widgets/pagelist.zig` -> `crates/rustty-inspector/src/widgets/pagelist.rs:283` (decode into a temporary copy so an inspector read cannot silently undo compression); `benchmark/*` -> `crates/rustty-bench/src/scrollback_compression.rs` and `page_compression.rs`. |
| `f8c217e55` | terminal: track pending scrollback compression | **PORTED** (re-triaged 2026-07-31) | cited by 1 Rust file; pending-compression tracking. Guard: `pagelist_compress_drain_finishes_in_one_call`. |
| `0d015b2fc` | terminal: preserve compressed pages during search | **PORTED** (WS7; row corrected `P3-W10 T2`) | 10 provenance-tagged (`oracle 0d015b2fc`) sites across `crates/rustty-vt/src/search/`: `sliding_window.rs:152` (`Meta.rows`), `:634`, `:696`, `:899`, `:916`, `:918` `append_if_wrapped` / `:942` `append_page` (the split that inspects a preserved page instead of restoring it), `viewport.rs:253,294`, `active.rs:137`, `pagelist.rs:612`. **Guard:** `search::pagelist::tests::compressed_history_match_spanning_page_boundary_remains_compressed` (`search/pagelist.rs:619`). |
| `e7969ed43` | terminal: make PageList compression self-contained | **PORTED** (re-triaged 2026-07-31) | cited by 1 Rust file. Guard: `pagelist_owns_incremental_compression_state` (mutation-proved: dropping `page_compression.reset()` from `scroll` fails it). |
| `70e788f06` | terminal: add incremental scrollback compression | **PORTED** (re-triaged 2026-07-31) | cited by 2 Rust files. Guards: `pagelist_compress_incremental_converges_to_complete`, `pagelist_incremental_compression_bounds_inspected_pages` (mutation-proved: budget `MAX`->`MAX+1` fails it). |
| `9156ada16` | terminal: add cold-history compression pass | **PORTED** (re-triaged 2026-07-31) | cited by 2 Rust files. The cold-history pass and **all 21** of its oracle tests are ported — 19 of them only after `G-TESTS` (the live-oracle test audit) found them absent on 2026-07-31; see `.port/phase-3/TEST_AUDIT.md`. One accepted absence (`advances after allocation failure`) is a `FailingAllocator` seam with no rustty analogue, argued there. |
| `421fe8dab` | terminal: integrate compressed pages into PageList | **PORTED** (re-triaged 2026-07-31) | cited by 1 Rust file. Guards: `pagelist_compression_restores_through_page_access`, `pagelist_lazily_restores_compressed_history_made_active_by_resize` (mutation-proved via `Chunk::rows()`/`Node::rows` reading through the restoring accessor). |
| `f6fd4cb08` | terminal: generalize page memory reclamation | **PORTED** (re-triaged 2026-07-31) | cited by 1 Rust file; page-memory reclamation generalized in `pagelist/`. Guard: `pagelist_prune_reuses_malformed_compressed_page_memory`. |
| `ebc3ffd22` | terminal: add compressed page representation | **PORTED** (re-triaged 2026-07-31) | `crates/rustty-vt/src/compress/page.rs` (510 lines, 4 tests) — `CompressedPage` + `clone_buf`. Guard: `pagelist_preserved_page_keeps_compressed_storage` (mutation-proved: clearing `dirty` on the decoded copy fails it). |
| `c62c15984` | terminal: add standalone LZ4 block codec | **PORTED** (re-triaged 2026-07-31) | `crates/rustty-vt/src/compress/lz4.rs` (998 lines, 13 tests) — the standalone block codec. Guard: **G-LZ4** (`check_lz4_byte_exact.sh`) proves the encoder byte-identical to the oracle's on 7,976 differential points; 1,631 of them diverge under a naive port, so the gate is load-bearing, not decorative. |
| `9c9cf3e82` | renderer: avoid allocating when no active links | **PORTED** (WS10 T9, `684ac51`) | `Link::active` `link.rs:92` + the pre-allocation early return; guarded by a counting `#[global_allocator]` zero-allocation assertion `link.rs:488`. Row was stale — re-vetted `P3-W5`. |
| `60121a039` | Revert "termio: bound POSIX read-ahead on non-Darwin" | n/a | reverts `bed47168c`; net-zero pair |
| `896aca499` | terminal: return free-listed page memory to the OS | ported (WS8 T12/T13) | pagelist pool decommit; Miri exception registered (MIRI_EXCEPTIONS.md Category 2); verified by `examples/rss_probe.rs` on macOS (9.0 MiB returned vs 0.0 control). Linux arm not yet run |
| `c442eb4b3` | terminal: document the pool reuse zeroing invariant | **PORTED** (P3-W1) | `const _` size_of Row >= size_of usize next to `Row`, `assert!(cap.rows > 0)` in `init_buf`, layout/pool comments, stale doc fixes (T4) |
| `e44f5cb0f` | terminal: guard RefCountedSet zero-capacity lookups | **PORTED** | ref_counted_set.rs:825 + style.rs test (§1) |
| `8307349ec` | terminal: fix increaseCapacity growth from zero-capacity dimensions | **PORTED** (P3-W1) | `grow.rs` `adjust_field!` jumps to `Capacity::default()` from 0 + 3 tests incl. a bounded-retry control (T4) |
| `16e4b5e98` | terminal: track page ownership explicitly on pagelist nodes | **PORTED** (re-triaged 2026-07-31) | cited by 2 Rust files; explicit node page-ownership. Guard: `pagelist_memory_stats_accounts_compressed_and_resident_bytes`. |
| `b953bb346` | terminal: fix bitmap allocator chunk region sizing | **PORTED** | bitmap_allocator.rs:335 + 2 tests (§1) |
| `3ff6d08fa` | lib-vt: report OSC and Kitty color queries (#13239) | n/a-merge | constituents below |
| `14c829883` | terminal: report OSC color queries in lib-vt | **PORTED** | encode_rgb8/16 + color_for_xterm/kitty + has_terminal_query_color + osc reset fix + stream reporting + vt-c KATs (WS5 T9-T10) |
| `20a1bfa5f` | fix: pass RGB color inputs by pointer | **PORTED** | rustty_color_rgb_get by-pointer ABI (WS5 T2) |
| `bb0ac4c72` | termio: don't bridge pty reads while the parser is idle | **PORTED** | landed WITH `2f0e6659d` as one change (WS10 T20, `a788346`) — 110 lines that only exist on top of the ring they modify |
| `bed47168c` | termio: bound POSIX read-ahead on non-Darwin | n/a | reverted upstream by `60121a039` |
| `77190bd02` | terminal: handful of scroll region optimizations | **PORTED** (`P3-W5 T3`) | `Screen::cursor_scroll_region_up` + `_slow` in `screen.rs`, `Terminal::index`/`scroll_up` routing; carries `0bad91adb`'s 4th `invalidateNodeLayout` site, closing WS7 Phase C divergence #11 and re-vet row P3-RV3. 10 oracle tests + a 168-case fast-vs-slow differential + `.port/phase-3/mirihar-scroll-region-up`. |
| `cabbdee32` | Fix adjust-cursor-height regression (#13225) | n/a-merge | constituents `dac341cad`+`e8f3f6c43` — PORTED |
| `446f80f4e` | terminal: render state update optimizations | **PORTED** | WS10 T19 (`56dba9c`): `page::Mask` + `begin_update`/`end_update` + `pending_styles` + chunked update in render.rs; `rustty_render_state_{begin,end}_update` in vt-c + `include/rustty/vt/render.h`; renderer two-phase; three `screen-clone` render modes. Measured lock-hold 2.88x-11.19x. The `Terminal.zig` hunk is a refactor of T16's `printSliceFill`, which does not exist in rustty yet — `Mask` is landed for it; adoption is T16's. |
| `b5053153f` | terminal: log unsupported-input messages once per distinct value | **PORTED** (WS10 T7, `6d69269`) | `log_unsupported_once` + the 16-slot atomic dedup table, 8 call sites converted (`stream.rs:1619`, `:2214`, `:2324`, …). Row was stale — re-vetted `P3-W5`. |
| `8d663a76e` | terminal: release style refs per run in clearCells | **PORTED** (P3-W1) | `clear_cells` groups same-style runs and calls `release_multiple` once per run, ids read through the raw `*mut Cell`; ref-count-to-exactly-zero tests (T6) |
| `cb2d78587` | terminal: fill style-only cell runs in bulk in printSliceFill | **PORTED** (WS10 T16, `6cffd25`) | `Terminal::print_slice_fill` `terminal.rs:1319`. Row was stale — re-vetted `P3-W5`. |
| `300f42c7a` | terminal: handle CSI entry bytes inline in consumeUntilGround | **PORTED** (WS10 T13+T14, `ae48e3b`) | `Stream::csi_entry_byte` `stream.rs:1115` behind the `HAS_VT_RAW` gate `:289`. Row was stale — re-vetted `P3-W5`. |
| `083d9709b` | terminal: decode ASCII inline in SIMD scan for ESC | **PORTED** (WS10 T15, `ae05b31`) | `WidenAsciiStore` / `DecodeNonAsciiUntilControlSeq` incl. the mandatory `HWY_TARGET == HWY_SCALAR` arm, `crates/rustty-simd/cbits/simd/vt.cpp:204,232,208`. Row was stale — re-vetted `P3-W5`. |
| `e8f3f6c43` | font/sprite: regression test for cursor-height metric | **PORTED** | face.rs test (§1) |
| `dac341cad` | font/sprite: cursor height respects adjust-cursor-height | **PORTED** | face.rs:704-716 (§1) |
| `cb4c49fbf` | terminal: scalar UTF-8 decode consumes partial sequences cut by ESC | **PORTED** (verified P3-W1) | ALREADY present on `main` -- the ESC-boundary guard, both named byte sequences, and the 10k SIMD-vs-scalar differential. Re-proven by mutation; the P3-W1 plan text claiming it was absent is stale (T6) |
| `4c5b1d5d5` | bench: terminal-stream reads 64KiB chunks | **PORTED** (WS10 T10, `713cf22`) | `crates/rustty-bench/src/terminal_stream.rs:249` `[0u8; 64 * 1024]`, unbuffered reader. Row was stale — re-vetted `P3-W5`. |
| `cee35cabf` | terminal: skip style map update when SGR unchanged | **PORTED** | screen.rs:2752 (§1) |
| `253e4f9c3` | terminal: bulk-parse CSI parameter bytes at slice level | **PORTED** (WS10 T13, `ae48e3b`) | `Stream::consume_csi_params` `stream.rs:1186`. Row was stale — re-vetted `P3-W5`. |
| `1a88f3622` | terminal: dispatch CSI finals directly from stream fast paths | **PORTED** (WS10 T14, `ae48e3b`) | `Stream::csi_dispatch_final` `stream.rs:1146`. Row was stale — re-vetted `P3-W5`. |
| `47e26df60` | terminal: batch printed codepoint runs into direct row fills | **PORTED** (WS10 T16, `6cffd25` + `587e5ae`; termio arm `P3-W7 T6`, `8da32f4`) | `Terminal::print_slice` `terminal.rs:1177`, `Action::PrintSlice` + `next_slice` run scan `stream.rs:888`, `rustty_simd::lanes`. Guarded by `terminal_print_slice_differential_fuzz_vs_print` `terminal.rs:16604`. Row was stale — re-vetted `P3-W5`. |
| `258de36d1` | benchmark: terminal-stream uses full terminal handler | **PORTED** (WS10 T10, `713cf22`) | `terminal_stream.rs` uses `rustty_vt::stream_terminal`'s readonly handler; guarded by `terminal_stream_uses_the_full_terminal_handler`. Row was stale — re-vetted `P3-W5`. |
| `2f0e6659d` | termio: pipeline pty reads to overlap parsing with draining | **PORTED** | WS10 T20 (`a788346`): `io-gather` thread + 4x64 KiB ring with a raw-pointer slot handoff, proven SB+TB clean on `.port/phase-2/mirihar-ws10-pipeline` against three negative controls. The QoS hunk landed separately as T8 (`7725606`) and is now applied to both threads. |
| `63e75e86c` | lib-vt: many more color utility APIs (#13206) | n/a-merge | constituent below |
| `2970e9a2a` | lib-vt: many more color utility APIs | **PORTED** | color.rs/x11_color.rs + vt-c color.rs/lib.rs/types.rs + color.h/sgr.h + config delegation (WS5 T3-T6) |
| `4a7cabc4f` | lib-vt: color scheme report encoder (#13192) | n/a-merge | constituent below |
| `715ef6c15` | fix: max window clamp to monitor size (#13171) | n/a-merge | constituent `004c88e41` (gtk) |
| `f00e90694` | lib-vt: add color scheme report encoder | **PORTED** | device_status::encode_color_scheme_report + vt-c color_scheme.rs + rustty_color_scheme_report_encode + color_scheme.h + example (WS5 T7-T8) |
| `004c88e41` | fix: set max window clamp to current monitor size | n/a-tier BL-8 | src/apprt/gtk — tier unported |
| `65e61282a` | lib-vt: add unicode grapheme width API | **PORTED** | grapheme::grapheme_width_effect/grapheme_width (print-path shared) + vt-c unicode.rs + rustty_unicode_grapheme_width + unicode.h (WS5 T12-T13) |
| `688750903` | Kitty dnd parser (#13029) | ported (n/a-merge) | constituents landed WS8 T2-T4 |
| `cca51729a` | lib-vt: scroll-to-row viewport scrolling (#13179) | n/a-merge | constituent below |
| `3a2e28329` | lib-vt: add scroll-to-row viewport scrolling | **PORTED** | ScrollViewport::Row @ terminal.rs + scroll_viewport mapping + vt-c Row tag/union + RUSTTY_SCROLL_VIEWPORT_ROW terminal.h (WS5 T14-T15) |
| `fc5a72772` | lib-vt: add unicode codepoint width API | **PORTED** | rustty_unicode::codepoint_width + vt-c unicode::codepoint_width + rustty_unicode_codepoint_width + unicode.h (WS5 T11-T13) |
| `0a410f18e` | terminal: clipboard_set effect for OSC 52 writes | **PORTED (subsumed)** | superseded by `634ef7198`; ported directly to the protocol-neutral clipboard_write shape (WS5 T16-T17) |
| `8fe5913f7` | kitty/gfx: fix build without 64-bit atomics | ported (WS8 T14) — N/A | every rustty target reports `target_has_atomic="64"` (`rustc --target <t> --print cfg`, checked for aarch64/x86_64 apple-darwin, x86_64/aarch64 unknown-linux-gnu, wasm32-unknown-unknown), so upstream's mutex fallback is not applicable; recorded as a module PORT NOTE |
| `bdc0b6c19` | kitty/gfx: generation stamps, delete transmit time | ported (WS8 T14/T15/T16) | vt core + renderer + vt-c/header/example; `grep -rn transmit_time crates/` → 0 code hits |
| `f245cdc66` | lib-vt: expose selection gesture to Zig | **PORTED (already covered)** | Zig-module re-export only; vt-c `rustty_selection_gesture_*` + rustty-vt `selection_gesture` module already live (WS5 T18 verified) |
| `aea63d71f` | libghostty: fix utf-8 grapheme length overflow | pending-small BL-3 | rustty-vt-c render seam (79/20, has tests) |
| `e5b65a2ce` | config: gtk-horizontal-tab-scroll option (#12659) | n/a-merge | constituents = BL-8 |
| `0a117e079` | gtk: fix missing dbus connection causing crash | **PORTED (`P3-W11 T4`)** | the crash-safety half was already present and independently derived (`global_shortcuts.rs` hoists the subscription drops above the connection check; `request()` returns instead of unwrapping). `P3-W11 T4` lands the tail: `close()` now calls `clear_global_shortcuts()` unconditionally. **DELIBERATE DIVERGENCE (kept):** upstream's `refresh()` also returns early when there is no D-Bus connection; rustty still builds the shortcut map headlessly, because two KATs assert that (`global_shortcuts_refresh_builds_map_without_dbus`, `…_clears_prior_map`) and upstream's early return is only an optimisation with no observable difference once a bus exists |
| `88bd4fdce` `c3b5cab94` `7ee3ac9ec` `0075c75b6` `3024c5d19` | gtk: `vicinae-hotkey-v1` global hotkeys (F-W1..F-W4) | **PORTED (`P3-W11 T4`)** | `protocols/vicinae-hotkey-v1.xml` vendored verbatim (305 lines, MIT © 2026 Aurelien Brabant, header intact); `wayland-scanner` `generate_interfaces!`/`generate_client_code!` in `src/gtk/winproto/wayland/vicinae.rs`; `winproto::App::{bind,clear}_global_shortcuts` with `noop`/`x11` false arms; `Globals` `Tag::VicinaeHotkeyManager`; `winproto/wayland/hotkeys.rs`; `GlobalShortcuts::BindFailed` + `bind-failed` + `emit_trigger`/`emit_bind_failed`; `Application::global_shortcut_bind_failed` → desktop notification + its 3 msgids. **ADDITIVE WITH PREFERENCE** — the XDG portal path is untouched and still required; the choice is per-session at runtime on whether the compositor advertises `vicinae_hotkey_manager_v1`. **The protocol is unratified/testing-phase; a future incompatible bump is expected.** Two forced divergences, both documented in `hotkeys.rs`: (a) `Hotkeys` lives in `Globals` and the per-proxy userdata is a `HotkeyId` token, because wayland-rs dispatches through one queue-state type with `Send + Sync` userdata (the reference's `*Entry` listener userdata has no Rust equivalent); (b) `trigger`/`bind-failed` emissions are DEFERRED out of the queue-dispatch `RefCell` borrow, because a `trigger` runs an arbitrary keybind action and `toggle_quick_terminal` re-enters the same `RefCell`. Wire-path verification debt: `VERIFICATION_DEBT.md` **V22** |
| `5fa37f2b7` | correct type for remaining rows in PageList.scroll | **PORTED** | pagelist/scroll.rs:135,166 (§1) |
| `c488ccda6` | terminal: correct OSC 72 doc comments | ported (WS8 T2-T3) | — |
| `a6132c18b` | terminal: OSC 72 EventType/Option doc comments | ported (WS8 T2-T3) | — |
| `dd6c09bcf` | terminal: clean up OSC 72 parser comments, add tests | ported (WS8 T2-T3) | — |
| `91c87e2cf` | terminal: parse kitty drag and drop protocol (OSC 72) | ported (WS8 T2-T3) | `crates/rustty-vt/src/osc/parsers/kitty_dnd_protocol.rs` |
| `b5f5ca078` | Add "available since" | ported (P3-W9 T3) | **ROW CORRECTED.** Was `n/a-tier BL-8 · Config docs for gtk option below` — wrong, for the same reason as the row below. `crates/rustty-config/src/config_struct.rs` `gtk_horizontal_tab_scroll` doc block, `Available since 1.4.0.` |
| `5ea45919d` | config: add gtk-horizontal-tab-scroll option | ported (P3-W9 T3), config half | **ROW CORRECTED.** Was `n/a-tier BL-8 · gtk tier unported; Config option lands with that tier` — wrong: the `Config` **field** has no GTK-apprt dependency. `Config` is a data structure, not a runtime, and rustty-config already carried sibling GTK options (`config_struct.rs` `gtk_wide_tabs`). Deferring the field to the GTK tier silently broke the field-count contract's claim to match the oracle (area G OQ-6). Field + 12 registration sites + 5 goldens: `crates/rustty-config/src/config_struct.rs`, `c_get.rs`. The commit's **behaviour** half (`src/apprt/gtk/class/surface.zig`, +61/−21) does belong to the GTK tier and is `P3-W8 T5 step 3`. |

---

## 3b. BL-4 rows ported by WS10 T11/T16/T17 (beyond-pin, 2026-07-30)

These strike their §3 rows above. Every one is beyond the pin, so the fidelity
target is the named commit read with `git -C ../ghostty show`, not the pinned
working tree (`.port/ORACLE_SYNC.md`'s pin is unchanged).

| upstream | subject | rustty commit | notes |
|---|---|---|---|
| `7e14347c1` | terminal: bound page map probe lengths | `f1730b2` + `b8d24a6` | `hash_map.zig` +472 (the real substance), `page.zig`/`hyperlink.zig`/`Screen.zig` hunks, and `benchmark/HyperlinkMap.zig` |
| `47e26df60` | terminal: batch printed codepoint runs into direct row fills | `6cffd25` | |
| `cb2d78587` | terminal: fill style-only cell runs in bulk in printSliceFill | `6cffd25` | fused with `47e26df60` — it rewrites that commit's loop interiors, see the commit message |
| `f2a7652ab` | mitchell's touchups | `587e5ae` + `6cffd25` + `f1730b2` | IN SCOPE ONLY IN PART — see the hunk-by-hunk sweep below |
| `f6f79acce` | terminal: dispatch APC string bytes in bulk slices | `f96957c` | incl. `synthetic/Kitty.zig`, `synthetic/cli/Kitty.zig`, `benchmark/ApcParser.zig` |
| `8c523ed03` | terminal: vectorize APC payload scanning | `f96957c` | |

### `f2a7652ab` hunk-by-hunk (35 files; its subject reads as pure cleanup and is NOT)

PORTED:
* `src/simd/main.zig` — `lanes(T)` -> `rustty_simd::lanes` (`587e5ae`).
* `src/terminal/Terminal.zig` (+155) — the manual `@Vector` fill/scan loops
  (`6cffd25`).
* `src/terminal/stream.zig` — the lane-wise printable-run scan (`6cffd25`).
* `src/terminal/hash_map.zig` — the force-inline of the auto context's hash.
  Ported as `#[inline]` on `AutoContext::hash` (`f1730b2`+); the
  `hasUniqueRepresentation` split selects between two Zig std hashers and has no
  counterpart in a port with one.

NOT APPLICABLE — each checked against the rustty source, not assumed:
* `src/quirks_memset.zig` (+322) — **must NOT be ported**, see
  `VERIFICATION_DEBT.md`'s `quirks_memset` caveat and the WS10 plan's PREMISE
  CORRECTION §1.
* `src/unicode/props.zig` + `src/unicode/grapheme.zig` — explicit `_padding` to
  give the packed structs a power-of-two backing integer, a workaround for Zig
  0.16 lowering loads/stores through exotic-width LLVM integers (`i9`/`i4`).
  rustty's `Properties` (`crates/rustty-unicode/src/props.rs:41`) and
  `Precompute::Value` (`grapheme.rs:220`) are ordinary Rust structs with natural
  field layout; rustc never emits the exotic-width accesses, so the workaround has
  nothing to work around.
* `src/config/path.zig` — this is a real BUG FIX (`realPathFile(io, ".", &buf)`
  -> `realPathFile(io, path, &buf)`, `dir_len` -> `abs_len`), but the bug was
  INTRODUCED after the pin by upstream's `std.Io` migration. At `4749c4e93` the
  line is already `dir.realpath(path, &buf)`, which is what rustty ported. No
  rustty-side defect.
* `src/benchmark/TerminalStream.zig` — "avoid buffers to avoid a memcpy"
  (unbuffered `f.reader(io, &.{})`). rustty's `options::read_slice_short` already
  reads straight from the `DataFile` with no intermediate reader buffer; the PORT
  NOTE at `crates/rustty-bench/src/terminal_stream.rs` already records this.
* `src/build/Config.zig` (`omitFramePointer`), `src/build/GhosttyExe.zig`,
  `GhosttyLib.zig`, `GhosttyLibVt.zig`, `LibsystemOverrideStep.zig`,
  `libsystem_override.sh` — Zig build-system. The frame-pointer half has no
  rustty action: rustc's aarch64-apple-darwin target spec already sets
  `frame-pointer = non-leaf` (Apple's ABI requires x29), which is exactly what
  the hunk restores on the Zig side; no `-C force-frame-pointers` is needed and
  none is added.
* `src/global.zig` / `main.zig` / `main_bench.zig` / `main_c.zig` /
  `main_gen.zig` / `lib_vt.zig` / `benchmark/cli.zig` / `synthetic/cli.zig` —
  `InitOpts.tool` plus the `global.io()`-before-init UB fix. rustty-bench's
  `main()` uses the process `Global` allocator with no `global.init` at all, and
  rustty-synthetic writes through its own `StdoutWriter`; there is no
  global-init ordering to get wrong.
* `build.zig.zon`, `build.zig.zon.json/.nix/.txt`, `flatpak/zig-packages.json`,
  `nix/build-support/update-mirror.nu`, `pkg/*/build.zig.zon` — Zig dependency
  lockfiles.

---

## 3c. Beyond-pin rows ported/verified by `P3-W1` (2026-07-30)

Two `P3-W1` commits have no §3 row because they postdate that table's window.
Recorded here so the ledger is complete. As in §3b, the fidelity target is the
named commit read with `git -C ../ghostty show`; the oracle pin
(`.port/ORACLE_SYNC.md`) is UNCHANGED — advancing it is `P3-W10`.

| upstream | subject | status | notes |
|---|---|---|---|
| `cb2fef390` | terminal: preserve underline style in DECRQSS | **PORTED** (P3-W1 T6) | `terminal.rs` `print_attributes` emits `;4:{n}` for non-Single underline; 5-case table test that retains the `Single` case so it cannot pass with the branch taken unconditionally |
| `5caf20e3e` | terminal: avoid reallocating tabstop storage | **PORTED** (verified P3-W1 T6) | ALREADY present on `main`: `tabstops.rs` reads `size <= self.dynamic_stops.len()` and `tabstops_resize_to_existing_capacity_does_not_allocate` exists and asserts an ALLOCATION COUNT, not merely success. Re-proven by mutation. The `P3-W1` plan text claiming both were absent is stale |

## 4. Standing ritual (R12)

Re-run this triage per sub-project completion and at minimum whenever the pin conversation resumes:
1. `git -C /Users/gael/projects/ghostty fetch origin` (fetch ONLY — checkout stays read-only, R3).
2. `git log --oneline <last-triaged-head>..origin/main -- src/ include/`; append new rows here.
3. Verdict each commit with file:line evidence; port small clear fixes WITH their test hunks; structural work gets/extends a BL-n entry.
4. Last triaged head: **`53bd14fec`** (2026-07-10, this pass).
