# Phase 4 — Area A re-vetting assessment: terminal core

Oracle range: `6ad1fe7d8` (Phase-3 pin, 2026-07-29) → `12967b68f` (= origin/main, 2026-08-17).
Oracle working tree is checked out AT the new pin, so all "head" reads are direct file reads.
Rustty side: `main` @ `8120dd10`, `crates/rustty-vt/src/`.
Author: area-A agent. Date: 2026-08-17.

Area-A file set (from the task brief): `PageList.zig`, `Terminal.zig`, `Screen.zig`,
`page.zig`, `style.zig`, `Selection.zig`, `Tabstops.zig`, `ScreenSet.zig`, `StringMap.zig`,
`ref_counted_set.zig`, `bitmap_allocator.zig`, `size.zig`, `mem.zig`, `fastmem.zig`,
`hash_map.zig`, `src/fastprint.zig`, `fraction.zig`, `stream.zig`, `stream_continuation.zig`,
`stream_terminal.zig`, `main.zig`, `render.zig`, `Parser.zig`, `parse_table.zig`,
`sanitize.zig`, `point.zig`, `cursor.zig`.

Files in the set with **no range commits**: `Selection.zig`, `size.zig`, `mem.zig`,
`fastmem.zig`, `parse_table.zig`, `sanitize.zig` — nothing to assess there
(verified: they do not appear in any commit of
`git log --oneline --no-merges 6ad1fe7d8..12967b68f -- <area paths>`).

---

## 1. Summary

**53 distinct non-merge commits** touch this file set (exact list = the git log
invocation above; reproduced per-commit below). Verdicts:

| verdict | count |
|---|---:|
| **ABSENT** | **49** |
| PARTIAL | 0 |
| PORTED | 0 |
| N-A | 4 |

**Nothing from this range has been ported yet** — expected, since Phase 4 porting has
not begun; this assessment is the pre-port re-vet. I verified this is not an
assumption: every "ABSENT" row below carries a rustty grep/read that shows the
pre-range shape still in place, and I explicitly probed the places where rustty's own
prior perf work could have coincidentally landed an upstream shape (it did not — see
§1.1). The Phase-3 backlog (printSlice family, `page::Mask`, tripwire fail points,
BL-3 fixes) **is** landed, so the pre-range baseline is clean.

Changed-line mass in my files, from `UPSTREAM_DIFF_STAT.txt`: PageList 2,112 ·
stream 1,036 · stream_terminal 717 · stream_continuation 606 (NEW) · Terminal 366 ·
style 218 · page 210 · render 205 · fraction 142 (NEW) · bitmap_allocator 109 ·
ref_counted_set 87 · fastprint 68 (NEW) · Tabstops 42 · cursor 34 · Parser 29 ·
hash_map 24 · Screen 21 · StringMap 14 · point 10 · ScreenSet 6 · main 3 ≈ **6,265 lines**.

### 1.1 Coincidence probes (why 0 PORTED is a measured result, not an assumption)

- rustty's own perf project introduced `use_multiple`/`add_with_id` batching — but only
  in the **page clone** path (`page_impl.rs:1212–1218`), not the reflow path the range
  optimizes. `grep -rn "bulk_copyable\|bulk_run_length\|copy_run\|eql_any" crates/rustty-vt/src` → 0.
- rustty `print_slice_fast` (`terminal.rs:1249–1355`) contains the status-display/
  modes/charset/hyperlink checks **inside the per-attempt fast path** — that is the
  pre-range upstream shape (verified against `git show 6ad1fe7d8:src/terminal/Terminal.zig`);
  `3d9b2b483` hoists them into `printSlice`. Not a partial port.
- rustty `page.rs:1602` has `Mask` (Phase-3 `446f80f4e`) but **without** `0fb3565c7`'s
  `eql_any`/nested field paths (`grep eql_any` → 0).
- rustty has `rustty-simd` with `base64.rs` — the SIMD decoder `c992658b2` needs exists,
  but the OSC-52 call site (`stream_terminal.rs:2384`) still uses the scalar
  `std_base64_standard_decoder`.

### 1.2 The high-severity subset (read first)

Four ABSENT commits are **live bugs / a live security exposure in rustty today**, all
small, none blocked on other areas:

| commit | rustty site | severity |
|---|---|---|
| `33d34cf5c` VS15 cursor underflow | `terminal.rs:2208` `cursor_cell_left(prev_left - 1)` — when the wide base is directly under the cursor, `prev_left == 0` → `0 - 1` on `CellCountInt`: **debug panic, release wraps to an out-of-bounds cell pointer** (same class as the Phase-3 BL-3 crashes; Rust is strictly worse than Zig here) | crash / OOB write |
| `d5c7e54ae` hyperlink-reflow string capacity | `pagelist/resize.rs:1652` still does the single combined `additional_required_string_capacity` test-alloc; `PageEntry.dupe` allocs URI and explicit ID **separately** with 32-byte chunk rounding, so dupe can fail after the check passed → upstream hit `unreachable` (ghostty #13522); rustty's equivalent path will panic the same way on resize with a near-full string allocator | crash on resize |
| `38e891e6c` title-report opt-in | `stream_terminal.rs:1431` `Csi21T` arm echoes the (attacker-controlled) title to the pty **unconditionally** whenever a `write_pty` effect is registered — the exact command-injection exposure upstream fixed. rustty has no `title_report` gate (`grep title_report` → 0) | **security** |
| `727b8a02f` bound grapheme allocations | no `grapheme_max_len` cap in `page.rs`/`page_impl.rs` (`grep grapheme_max_len` → 0): unbounded zero-width codepoints per cell keep forcing grapheme-capacity doublings — easy memory DoS. (The 8 MiB OSC cap half lives in `osc.zig` = area D; the caps must land coherently but are separable.) | DoS |

Behavior-correctness (non-crash) fixes also live-wrong in rustty today:
`bfd40c84b` (CSI 2 K leaves `wrap` set → wrong reflow joins after resize;
`terminal.rs` `EraseLine::Complete` arm has no `cursor_reset_wrap()`), and the
test-side halves of `7cd2f65f5` / `c092b2bcf` (behavior halves owned by area D).

---

## 2. Per-commit verdicts, by cluster

Legend: cost **S** ≤ ~50 lines Rust, **M** ~50–400, **L** > 400 or cross-crate ripple.
"Upstream tests" = new `test` blocks the commit adds **in my files** (counted by
`git show <c> -- <my paths> | grep -c '^+test '`); these are the regression carriers
the port must bring.

### 2.1 PageList Builder / allocatePage (snapshot-enabling APIs)

These exist to serve `src/terminal/snapshot/` (decode builds a PageList from raw
pages) — the snapshot subsystem itself is another area, but these APIs are mine and
are useful/testable standalone.

| commit | what | verdict | evidence | tests | cost |
|---|---|---|---|---|---|
| `fc1bd06a1` PageList Builder to build from raw pages (+289) | new `pub const Builder` — accumulates caller-owned raw pages, validates, transactional `finish()` into a live PageList | **ABSENT** | `grep -rn "struct Builder" crates/rustty-vt/src/pagelist/` → 0 | 3 (`Builder transfers mixed-width pages`, `validates the finished list`, `finish is transactional on allocation failure`) | **M/L** |
| `35db32078` PageList allocatePage (+331) | new `pub fn allocatePage` + `pub const PageAllocation{destination,node}` with `page()`, `finalize(Location)`, detached-until-finalize semantics, limit checks before mutation | **ABSENT** | `grep -rn "allocate_page\|PageAllocation" crates/rustty-vt/src/pagelist/` → 0 | 4 (`finalizes pages and preserves live state`, `stays detached until finalize`, `rejects limits before modifying the destination`, `allocation failure leaves list unchanged`) | **M/L** |
| `457c5a0a6` align Builder/PageAllocation APIs (+40/−34) | renames `Builder.addPage`→`allocatePage`, `PageAllocation.cancel`→`deinit`; `node` becomes optional so `deinit` is safe after `finalize` (unconditional deferred cleanup) | **ABSENT** — port the **end state** of all three commits; do not pass through the `cancel` form | same greps | 0 | folds in |

**Atomic bundle A-1:** `fc1bd06a1` + `35db32078` + `457c5a0a6` — port as one end-state
change carrying all 7 tests. **Coupling:** the snapshot area consumes them
(`snapshot/{page,grid,terminal}.zig`); land before or with it.

**SB hazard:** `PageAllocation` holds `destination: *PageList` + `node` while the
caller populates `page()` — in Rust this must be raw `*mut PageList` (the caller will
also touch the list); `Builder` takes ownership of caller-provided page buffers
(from the C API eventually) — provenance must be carried via raw pointers, not
`&mut` (LIFETIMES.tsv row required per adopt-lifetimes-tsv).

### 2.2 Reflow: bugfix + perf chain (the "+2,100 PageList churn")

Upstream rebuilt `ReflowCursor` around a masked bulk-copy fast path. rustty's
`pagelist/resize.rs` has **none** of it (`grep bulk_run_length\|copy_run\|row_has_pins\|recycle` → 0);
its reflow is the faithful pre-range per-cell state machine.

| commit | what | verdict | evidence | tests | cost |
|---|---|---|---|---|---|
| `d5c7e54ae` **fix** string capacity check in hyperlink reflow | new `ReflowCursor.hyperlinkStringsFit` mirroring `PageEntry.dupe`'s exact two-allocation pattern; re-checked after `increaseCapacity` swapped `self.page` (second dupe site) | **ABSENT — live crash, see §1.2** | `resize.rs:1652` pre-fix combined check; `grep hyperlink_strings_fit` → 0 | 1 (`…hyperlink dupe string alloc chunk rounding`) | **S/M** — do this first, independent of the perf chain |
| `4a88cc594` skip reflow pin scans for rows without pins | hoists a per-row `row_has_pins` computed once from `tracked_pins.keys()` | **ABSENT** | no `row_has_pins` in `resize.rs` | 0 | **S** |
| `c5ca2db1b` memoize style id mapping during reflow (+49) | per-(src page, style id) memo of dst style id; invalidated on page swap | **ABSENT** | — | 0 | **S/M** |
| `c249b9de3` bulk-copy runs of simple cells (+167) | `bulkCopyable`, `bulkRunLength` (via `page.Mask` pattern compare), `copyRun` (memcpy + `styles.useMultiple`/`addWithId`) | **ABSENT** | — | 0 | **M** |
| `179161c08` memoize reflow new-page capacity adjustment | `computeAndMemoizeCap` — cache the adjusted `Capacity` per source-page capacity | **ABSENT** | — | 0 | **S** |
| `46276d046` recycle pages within a column reflow (+71) | same-cols reflow reuses the **source node's own page memory** for the destination (re-`layout` + `initBuf` + memset) instead of alloc/free churn; `destroyNode` avoided | **ABSENT** | — | 0 | **M** — top SB hazard, see §4 |
| `0fb3565c7` Mask: nested field paths + `eqlAny` (+98, `page.zig`) | `Mask` accepts `"a.b"` field paths; new `eqlAny` (any-of-N patterns per group) | **ABSENT** | rustty `page.rs:1602` Mask has `eql_prefix`/`match_scalar` etc. but no `eql_any`, no nested paths | 1 (`Mask nested field path`) | **M** — const-generic expansion per Phase-3 §5(e) precedent |
| `d4e446c48` reduce reflow run scan to masked compares (+57) | run scan rewritten on `Mask.pattern`/`eqlScalar` | **ABSENT** | — | 0 | **S/M** |
| `ec5b36961` vectorize reflow run scan (+20) | group-wise `eql`/`eqlAny` scan (needs `0fb3565c7`) | **ABSENT** | — | 0 | **S** |
| `88ed6bebf` much faster wide-character reflow (+429) | run may start on a wide pair; three patterns (narrow/wide/spacer-tail) per proto; odd-cols spacer-head handling; degenerate spacer-tail style handling | **ABSENT** | — | 5 (`…wide char bulk run{,…odd cols spacer head,…mixed narrow round trip,…styled,…degenerate spacer tail style}`) | **M/L** |

**Atomic bundle A-2 (ordered):** `c249b9de3` → `d4e446c48` → `0fb3565c7` →
`ec5b36961` → `88ed6bebf` rewrite the **same scan/copy functions**; port the END
STATE once (head `PageList.zig` `ReflowCursor.bulkRunLength`/`copyRun`) and carry all
6 tests. `4a88cc594`/`c5ca2db1b`/`179161c08`/`46276d046` are separable but live in the
same functions — cheapest landed together. `d5c7e54ae` is independent and urgent.
Note: upstream test `"PageList Pin row movement clamps across mixed-width pages"`
appears in the range diff only because it **moved**; it pre-exists at the pin and is
already in rustty (`pagelist/tests.rs:15104`) — do not double-port.

### 2.3 Stream continuation / ground-state (NEW `stream_continuation.zig`; the stream +1,036)

| commit | what | verdict | evidence | tests | cost |
|---|---|---|---|---|---|
| `68beeeb3f` stream continuation tracking for replay (+1,509 across stream/stream_continuation/stream_terminal/Terminal) | opt-in tracking on `Stream`: backwards vectorized ESC search (`findVTReplayStart`), UTF-8 lead-byte search (`findUtf8ReplayStart`), suffix normalization (drop already-handled BEL etc.), byte cap + broken-state recovery, `writeContinuation`; `Terminal.vtStream` switches to `Stream.init(.{allocator,handler})`; stream_terminal effect-dedup on replay | **ABSENT** | no `stream_continuation.rs`; `grep -n continuation crates/rustty-vt/src/stream.rs` → 0 | **19** (10 in `stream.zig` "continuation …", 8 in `stream_continuation.zig` boundary-scanner/tracker tests, 1 in `stream_terminal.zig`) | **L** |
| `70e41e96d` snapshot pty continuation (+66 in `stream_continuation.zig`) | `ValidateError{NoPendingState,…}` + validation that a continuation is minimal/non-ground | **ABSENT** | — | 0 (tests live in snapshot area files) | **S** |
| `a69a591af` detect/write until stream ground state (+196) | `pub inline fn nextSliceUntilGround` (+ untracked variant): consume the shortest prefix reaching ground | **ABSENT** | `grep until_ground` → only the pre-existing internal `consume_until_ground` | 5 (`nextSliceUntilGround …` ×5) | **M** |
| `d7bb4b863` C API for snapshotting (my side: 1 line) | `fn ground` → `pub` (parser ground **and** UTF-8 decoder state 0) | **ABSENT** — rustty has no `ground()` helper at all; folds into the continuation end-state | — | 0 | folds |
| `afb351f83` fast-path APC termination (+32) | after bulk APC consume, handle ESC / C1 ST (0x9C) inline instead of re-entering the generic action loop | **ABSENT** | `stream.rs:1023` `consume_apc_string` return falls through to the scalar path; no 0x9C fast-path | 1 (`apc bulk slice C1 ST`) | **S** |
| `b53728241` report unknown APC sequences (my side: stream.zig +18, main.zig +1) | `apc_end` action gains payload `ApcEnd{terminated:bool}` (false on CAN/SUB/abort); `main.zig` re-exports `UnknownSequence` | **ABSENT** | rustty `stream.rs:615` `fn apc_end(&mut self)` takes no argument | 1 (in stream_terminal: `unknown APC effect callback`) | **S/M** — **signature change** ripples into `apc` handler (area D) and vt-c |

**Atomic bundle A-3:** `68beeeb3f`+`70e41e96d`+`a69a591af`+`d7bb4b863` are one
feature; `afb351f83`+`b53728241` rewrite the same APC exit path — port the APC exit's
END STATE (`terminated` flag included) once. **Couplings:** `c/io.zig` + `TinyIo` +
snapshot (other areas) consume continuation; `apc.zig` (area D, G1b apc drop=4)
consumes `ApcEnd`; C header `include/ghostty/vt/io.h` (area C).

**These carry the G1b `stream` module RED (drop=8)** — see §5.

### 2.4 fastprint / formatter support (NEW `src/fastprint.zig`; pin-map redesign)

| commit | what | verdict | evidence | tests | cost |
|---|---|---|---|---|---|
| `8838c37f4` fast print styles | NEW `src/fastprint.zig` (`printDecimal` u8) + `style.zig` VT/CSS color emission assembled in stack buffers (`formatColorRgb`, `appendColor`) instead of `writer.print` | **ABSENT** | no `fastprint.rs` anywhere (`ls crates/*/src` + grep); `style.rs:627–670` still `write_str`/`format_color` per sequence | 0 | **M** |
| `79aa256fa` speed up formatter avoiding std.fmt (my side: fastprint +17) | `printDecimal` u21 arm | **ABSENT** | — | 0 | **S** (my side; formatter.zig half is area D — G1b formatter drop=2) |
| `d4391ff83` fastprint: fix compile errors | `v: u8` → `v: T` + the `test printDecimal` | **ABSENT** — port end state of the three | — | 1 | folds |
| `2ed67cadd` redesign pin map for formatter (my side: Screen +9, StringMap 14, style 143) | `StringMap.map` becomes `PinMap.Map` (offset→Pin map defined in `formatter.zig`, area D); `Screen.selectionString` builds it and moves ownership; `style.zig` `VTFormatter.format` fully buffer-assembled (flags + underline + colors in one `writeAll`) | **ABSENT** | `string_map.rs:61` still `pub map: Vec<Pin>`; `screen.rs:4228` `let mut pins: Vec<Pin>`; `style.rs` per-sequence writes | 0 | **M** — **atomic with area D's `formatter.zig` PinMap definition** |

**Atomic bundle A-4:** fastprint trio + `2ed67cadd` + area D's `79aa256fa`/
`997a2aff2`/`e523cf810` formatter work. Where the file boundary crosses
(PinMap type in `formatter.zig`, consumed by my `StringMap`/`Screen`), one landing.
Note the brief's warning about `997a2aff2` (pending-wrap) and `e523cf810` (cursor
home after tabstops): **both touch only `formatter.zig`** (verified `--stat`), so
area D owns them entirely; my coupling is only that the formatter reads
`Screen` pending-wrap/cursor state through existing accessors — no area-A code change.

### 2.5 Terminal/stream behavior, security, and small fixes

| commit | what | verdict | evidence | tests | cost |
|---|---|---|---|---|---|
| `33d34cf5c` avoid VS15 cursor underflow | locate spacer tail from the wide **base** (`prev_x = cursor.x - prev.left`; `cells[1]` when `prev_x < cols-1`), reposition via `cursorHorizontalAbsolute(min(prev_x+1, right_limit-1))`, always clear `pending_wrap` | **ABSENT — live crash, §1.2** | `terminal.rs:2208` pre-fix `cursor_cell_left(prev_left - 1)` | 2 (VS15 wraparound-disabled / restored-pending-cursor) | **S/M** |
| `bfd40c84b` reset wrap state for CSI 2 K | `.complete` arm calls `cursorResetWrap()` (deliberate xterm divergence, reflow rationale in comment — keep the comment, R1) | **ABSENT** | rustty `EraseLine::Complete => (0, self.cols)` bare; `cursor_reset_wrap` exists (`screen.rs:2382`) but is not called there | 1 (`eraseLine complete resets wrap`, includes a resize regression) | **S** |
| `f973bd53b` report overline in DECRQSS SGR | `printAttributes` attrs become numeric `[9]u8` (adds `53`), `;{}` formatting; stream_terminal test | **ABSENT** | `terminal.rs:5375` still `[u8;8]` of ASCII chars, no overline | 0 new blocks (extends 2 existing tests — port the extended assertions) | **S** |
| `727b8a02f` bound OSC and grapheme allocations (my side: page.zig cap + tests) | `pub const grapheme_max_len = 64`; `setGraphemes` truncates, `appendGrapheme` ignores past cap; PageList grapheme-reflow test rewritten onto `setGraphemes`+`init('X')` | **ABSENT — DoS, §1.2** | no `grapheme_max_len` | 3 (2 in page.zig, 1 in Terminal.zig; + the rewritten PageList test) | **S/M** — OSC 8MiB half is area D (`osc.zig`) |
| `38e891e6c` require opt-in for title reports (my side: stream_terminal +35) | `Handler.title_report: bool = false`; `.csi_21_t` returns unless enabled | **ABSENT — security, §1.2** | `stream_terminal.rs:1431` unconditional echo | 2 (replaces `"size report csi_21_t title"` with `…disabled by default` + `…enabled` — **delete the old test**, R1) | **S** — C-API option half is area C (`c/terminal.zig` +65) |
| `4371871bc` kitty: evict without scratch allocation (my side: Screen 4, Terminal 4) | `kitty_images.setLimit` and `setKittyGraphicsSizeLimit` become infallible (`!void`→`void`; `catch unreachable` dropped) | **ABSENT** (my side is signature-only) | `terminal.rs:5302` still fallible per pre-range | 0 (mine) | **S** — **atomic with area D** (`graphics_storage.zig` −273 rewrite is theirs) and area C (`c/terminal.zig`) |
| `5b70f208b` print repeated chars through printSlice | `printRepeat` feeds 4096-cp chunks to `printSlice` | **ABSENT** | `terminal.rs:1193` still per-`print` loop | 0 | **S** |
| `51a4311ef` clean up overzealous inlining (my side) | `printRepeat` buffer filled by bounded loop, not `@splat` (33KB unroll) — port the END STATE of `5b70f208b`+this | **ABSENT** | same | 0 | folds |
| `74efadb44` answer XTGETTCAP queries (+211 stream_terminal) | DCS XTGETTCAP handler: hex-decode names, respond `DCS 1$r…` / `DCS 0$r`, TN special-case, requires write effect | **ABSENT** | `stream_terminal.rs:596` still `Command::Xtgettcap(_) => Ok(())` | 3 (`XTGETTCAP responses`, `…without write effect is ignored`, `…TN responses`) | **M** |
| `c992658b2` OSC 52 base64 via SIMD decoder | clipboard write path uses `simd.base64.maxLen/decode` | **ABSENT** | `stream_terminal.rs:2378–2384` scalar decoder; `rustty-simd/src/base64.rs` exists to consume | 0 | **S** |
| `7cd2f65f5` color reset sets override to null (my side: test hunk) | OSC 111 test asserts reset resolves through the **default** and follows later default changes | **ABSENT** (my side) | rustty OSC-11 test lacks the default-follow assertions | 0 new blocks (extends 1) | **S** — behavior half is `color.zig` (area D) |
| `c092b2bcf` report DECECM permanently reset (my side: test hunk) | DECRQM test adds `\x1B[?117$p` → `…117;4$y` | **ABSENT** | `grep 117 stream_terminal.rs` → 0 | 0 new blocks (extends 1) | **S** — mode-table half is `modes.zig` (area D, G1b modes drop=1) |
| `1fdbb8c91` -Dvt-features (my side: Terminal +2, stream_terminal +4) | `build_options.glyph_protocol` gates the glyph APC handler + 2 test skips | **ABSENT** | `build_options.rs` has `simd`/`kitty_graphics` but no `glyph_protocol` (grep → 0) | 0 | **S** — the `build_options.zig` (+254) flag machinery itself is another area; my side is the 3 gate sites |
| `ac4dc1205` size dynamic tabstops by bits | `dynamic_count = divCeil(cols - prealloc_columns, unit_bits)`; grow via `realloc`, `@memset` only the tail | **ABSENT** | `tabstops.rs` `resize` still treats `size = cols - PREALLOC_COLUMNS` as a **unit** count (8× overallocation) with alloc+copy+free | 0 new blocks (rewrites `dynamic allocations` test's assertions — port the new assertions) | **S** |

### 2.6 render.zig / wasm / ABI metadata / misc perf

| commit | what | verdict | evidence | tests | cost |
|---|---|---|---|---|---|
| `e3056658d` faster render state updates + C API reads (my side: render +156) | `RowData.applied_styles: ArrayList(StyleRun)` (general allocator, capacity reserved in `beginUpdate` because `endUpdate` cannot allocate); `endUpdate` skips identical style runs; cleared on cell-storage realloc | **ABSENT** | `grep applied_styles crates/rustty-vt/src/render.rs` → 0 | 1 (`endUpdate skips unchanged style runs`) | **M** — C-API read-path half is `c/render.zig`+`c/style.zig` (area C) |
| `74a233b54` faster render reads/updates on wasm (my side: render +31) | `fillStyles` manually-vectorized struct memset (vector-typed pointers; Zig 0.16 disabled LLVM loop vectorizer) | **ABSENT** | no `fill_styles` | 0 | **S/M** — the vectorization *reason* is Zig codegen; per Phase-3 precedent (`f2a7652ab`) port the semantics, benchmark, document |
| `0d37f2d34` dirty row iteration + clear (my side: render +10) | `pub fn clean()` — clears global dirty + all per-row dirty flags | **ABSENT** | no `fn clean` in render.rs | 0 | **S** — the iteration API itself is `c/render.zig` (area C) |
| `3d9b2b483` much faster grapheme-heavy IO (PageList +184, Terminal ±133, bitmap_allocator +109, hash_map +24, style +8) | (a) `increaseCapacity` grapheme-bytes **capacity projection** from current row-density instead of doubling; (b) bitmap allocator `search_start` hint; (c) wasm small-key hash alternative in `hash_map.zig`; (d) `printSlice` hoists fast-path eligibility checks out of `printSliceFast` | **ABSENT** (all four) | grow.rs no projection; `grep search_start bitmap_allocator.rs` → 0; hash_map.rs no wasm arm; `print_slice_fast` checks per-attempt (`terminal.rs:1249`) | 4 (2 `increaseCapacity …projects capacity…` in PageList; 2 bitmap/hash tests) | **M/L** |
| `88ed6bebf` | — assessed in §2.2 | | | | |
| `87f69a12e` faster vt_write on wasm (my side: stream +9, style +11) | per-byte loop becomes **debug-only** (batched path even when `build_options.simd` is false, via scalar fallbacks); `Style.eql` compares `PackedStyle` u128 on wasm only | **ABSENT** | `stream.rs` gate is still `debug or !SIMD` (header comment `stream.rs:15` documents exactly the old switch); `style.rs:338` eql has no wasm arm | 0 | **S/M** — wasm arms follow rustty's wasm-target decision (couple: wasm allocator area) |
| `492c26067` custom memory pool for wasm (PageList ±132) | `PagePool = WasmPagePool` on wasm (module-global free list, 1-item growth) else std pool; `reclaimPoolPage` | **ABSENT** | `grep -rn WasmPagePool crates/` → 0 | 0 | **M** — **blocked on datastruct area** (`wasm_page_pool.zig` NEW) |
| `946422dbc` custom fraction parsing | NEW `fraction.zig` (142): parse `0`–`1` simple decimals without `std.fmt.parseFloat`; `main.zig` test import. Consumers: `SelectionGesture.zig` (mine adjacent), `color.zig`, `apc/glyph*` (area D) | **ABSENT** | no `fraction.rs` | 1 (`test parse`) | **S/M** — pure new leaf; the *callers'* switchover lands with their areas; note Rust has no parseFloat-bloat motive but R1 wants the same parser semantics (reject forms parseFloat accepted) |
| `b61fd5fbb` iterator for ref counted set (+87) | `Entry{id, value_ptr}` + `Iterator{items: [*]Item, id, end}` — ascending-ID live entries; **any mutation invalidates** | **ABSENT** | `grep Iterator crates/rustty-vt/src/ref_counted_set.rs` → 0 | 1 (`iterator visits live entries in ID order`) | **S** — consumer is snapshot encode. SB: keep `items` a raw ptr in Rust; do NOT hand out `&mut` items while iterating |
| `9673a22b0` expand ABI type metadata (my side: Terminal 2, point 10, stream 2) | `lib.CUnion(…)` second arg becomes options struct `.{ .padding = …, .field_renames = … }` (Point renames all coord arms to `"coordinate"`) | **ABSENT** | rustty `point.rs:17` doctrine comment: `.zig`-target lib types collapse to plain Rust enums; the C manifest machinery lives in `src/lib/` = **area C**. My 3 call sites change only when area C ports `lib.c_abi` options | 0 | **S** (my side), blocked on area C |
| `0e8b7bea6` expose packed cell layout (page.zig +46) | `Cell.CLayout = lib.Packed(Cell, …)` — reflected bit offsets/widths incl. tagged-union content arms, `_pad`/`_padding` omitted | **ABSENT** | no `CLayout` analogue; blocked on area C's `lib/packed.zig` port | 0 (in my file; schema-verify tests are `c/` area) | **S/M**, blocked on area C |

### 2.7 N-A rows (each with the required reason)

| commit | reason |
|---|---|
| `b867a0f59` snapshot lib.Enum (Screen +8: `SemanticClickKind`; cursor.zig rewrite of `Style` onto `lib.Enum(lib.target,…)`) | **N-A.** rustty doctrine (documented at `point.rs:17`): `lib.Enum` for the `.zig` target IS a plain enum; rustty's `cursor.rs:12–21` already has the identical `Bar/Block/Underline/BlockHollow` variants and `screen.rs` the `SemanticClick` union. The C-ABI enum arm is area C's manifest work. No Rust structural delta exists to port. (Doc-comment reflow of the Style docs should ride along whenever cursor.rs is next touched — cosmetic.) |
| `627f34309` helpgen needs terminal options (cursor.zig 2 lines) | **N-A.** Changes `lib.Enum(lib.target,…)` → `lib.Enum(.zig,…)` + import path — i.e. it *reverts* `b867a0f59`'s target-dependence for this type within the range; the end state equals rustty's existing plain enum. Zig build-graph (helpgen) motivation has no Rust analogue. |
| `e20564791` spacer-tail handling respects slow runtime safety (1 line) | **N-A.** Zig fix targets mixed optimization modes (Debug dependency inside ReleaseFast root: `std.debug.runtime_safety` reads the ROOT's safety). Rust has no per-module safety split: rustty gates the same printCell fixup on `cfg!(debug_assertions)` (`terminal.rs:2448–2452`) and gates `assert_integrity` on the **same** predicate (`page_impl.rs:544`), so the fixup and the integrity check can never disagree — the invariant the commit restores holds by construction. rustty's mapping `slow_runtime_safety → cfg!(debug_assertions)` is already the established doctrine (`page_impl.rs:187–188`). |
| `0aa71d02e` reduce Parser.Action log formatting code size | **N-A.** Both halves are Zig `std.Io.Writer` logging-formatter code-size work (`printValue` depth 3→1; `noinline` cold `warnCsiSepMismatch`). rustty deliberately does not port the logging formatters (PORT NOTE `parser.rs:205` "logging-only formatter; no behavioral effect"; the CSI separator warn itself is commented out at `parser.rs:818`). No behavioral delta. |

No commit in this range was **reverted** within the range (checked: no `Revert`
subjects among the 53; `627f34309` partially supersedes `b867a0f59` for one type —
handled above as end-state).

---

## 3. Atomic bundles (summary)

| bundle | members | why atomic |
|---|---|---|
| A-1 Builder/PageAllocation | `fc1bd06a1`+`35db32078`+`457c5a0a6` | later commits rename/refactor the earlier APIs; port end state, carry 7 tests |
| A-2 reflow bulk path | `c249b9de3`+`d4e446c48`+`0fb3565c7`+`ec5b36961`+`88ed6bebf` (order matters; `d5c7e54ae` independent-first; `4a88cc594`/`c5ca2db1b`/`179161c08`/`46276d046` same-function riders) | successive rewrites of `bulkRunLength`/`copyRun`; only the end state is coherent |
| A-3 continuation | `68beeeb3f`+`70e41e96d`+`a69a591af`+`d7bb4b863`; APC exit end-state `afb351f83`+`b53728241` | one feature; `apc_end` signature change must land once |
| A-4 fastprint/formatter | `8838c37f4`+`79aa256fa`+`d4391ff83`+`2ed67cadd` **with area D's formatter half** | `PinMap` type defined in formatter.zig, consumed by my StringMap/Screen; fastprint consumed by both areas |
| A-5 printRepeat | `5b70f208b`+`51a4311ef` | second rewrites the first's buffer fill; end state only. Depends on nothing else (printSlice already ported) |
| A-6 render state | `e3056658d`+`74a233b54`+`0d37f2d34` with area C's `c/render.zig`/`c/style.zig` | `applied_styles`/`clean`/bulk-read APIs are one C-API story; my side is testable standalone (1 test) |

## 4. UB/SB hazards for rustty's raw-pointer structures (PORT_RULES seams)

1. **`46276d046` page recycling during column reflow — highest risk.** The reflow
   cursor re-initializes (`layout`+`initBuf`+partial `memset`) the **source node's own
   page memory** as the destination while `ReflowCursor` state and possibly tracked
   pins still reference the old contents. In Rust the src reads must provably complete
   before reuse; keep `*mut Page` for both roles, never a `&mut Page` overlapping the
   read span. Prove on the isolated Miri harness (`.port/phase-2/mirihar-chunk3b`
   precedent, per shaper-hook-sb-clean-interleave).
2. **`c249b9de3`/`88ed6bebf` `copyRun`** — memcpy from src page cells into dst page
   while calling `dst.styles.useMultiple`/`addWithId` (mutates dst page memory) with
   the src cell slice still live. Read src through `*const Cell` raw pointers (the
   existing reflow convention in `resize.rs`), not a `&[Cell]` derived from a `&mut`.
3. **`b61fd5fbb` RefCountedSet Iterator** — holds `items: [*]Item` into set memory;
   upstream doc: "any mutation invalidates". Rust port must hold a raw pointer +
   `next()` returning raw-derived `*mut T` (or a narrowly-scoped `&mut` created per
   item), and snapshot-encode (the consumer) must not grow the set mid-iteration.
4. **A-1 `PageAllocation`** — `destination: *PageList` held while caller mutates the
   list through other paths before `finalize`; raw `*mut PageList` + re-derive on use.
   `Builder` from raw C-provided pages: provenance via `NonNull<u8>` buffers, no
   intermediate `&mut [u8]` spanning a page another alias reads.
5. **`2ed67cadd` PinMap** — pins stored in a map are **tracked pins**
   (`tracked_pins` registration); rustty pins are `*mut Pin` — the map must own the
   raw pointers and untrack on deinit exactly as `Vec<Pin>` does today; ownership
   moves to `StringMap` (the `pins = .empty` reset) must not leave a double-untrack.
6. **`33d34cf5c`** — `cells: [*]Cell = @ptrCast(prev.cell)` then `cells[1]` — in Rust:
   `prev_cell.add(1)` on the existing `*mut Cell` (already raw at
   `terminal.rs:1987`), guarded by `prev_x < cols-1`; do NOT construct a 2-slice.
7. **`3d9b2b483` capacity projection** — lives inside `increase_capacity`
   (`grow.rs`), which already re-derives pages after realloc via raw projection
   (`grow.rs:720,807` PORT NOTEs); keep the projection math on copied scalars, not on
   a held `&Page`.
8. **Arithmetic discipline (R7):** `33d34cf5c` (`cursor.x - prev.left`,
   `prev_x + 1`), `ac4dc1205` (`divCeil`), reflow run lengths (usize vs
   `CellCountInt`) — every subtraction here was a crash class in Phase 3; use
   checked/saturating forms per the established precedent, never bare `-` on u16.

## 5. Dropped-test targets (G1b `w0/g1b-after-at-12967b68f.txt`) belonging to area A

- **`stream` RED, drop=8 — mine.** Upstream `stream.zig`: 44 tests at pin → 60 at head
  (+16, none removed). rustty has 51 `#[test]`s (7 port-specific extras over the old
  44 offset the raw arithmetic; the gate's apc-deferral adjustment yields 8). The
  **actual missing set is all 16 new upstream tests**, all from bundle A-3:
  `apc bulk slice C1 ST`; `nextSliceUntilGround {stops at the earliest boundary,
  consumes all input without a boundary, handles UTF-8 boundaries, handles aborts and
  bulk strings, tracks only the consumed prefix}`; `continuation {lifecycle, suffixes
  are replay safe, reconstructs every unfinished VT state, is chunking-independent and
  idempotent, rebuilds APC handler input, cap and recovery, spans multiple bulk feeds,
  exact cap and large unfinished string, allocation failure recovers, every-byte cuts
  preserve future behavior}`.
- **Masked-green warning (per the "derived-gate green can be bookkeeping" doctrine):**
  `stream_terminal` (zig 87 / rust 87 "ok"), `terminal` (412/432 "ok"), `pagelist`
  (283/300 "ok"), `page` (51/76 "ok") are green **only because rustty carries surplus
  port-specific tests**. Measured: `stream_terminal` gained 7 upstream tests in-range
  and lost 1 (`size report csi_21_t title` replaced) — none of the 7 exist in rustty
  (probed: no XTGETTCAP handler, no `title_report`, no continuation) — yet the module
  reads "ok". Same for Terminal's 4, PageList's 16 (one apparent 17th,
  "Pin row movement clamps…", only moved — already in rustty), page's 3
  (Mask nested + 2 grapheme caps), ref_counted_set's 1, render's 1, fraction's 1,
  fastprint's 1, stream_continuation's 8 (module not even in the gate's list yet —
  a NEW module the gate must learn to count). **The finale G1b must not be trusted
  alone for these modules; the per-commit test lists in §2 are the checklist.**
- Area-adjacent REDs owned by others but coupled to my bundles: `formatter` drop=2
  (area D; A-4), `modes` drop=1 (area D; `c092b2bcf`), `apc` drop=4 (area D; A-3).

## 6. Coverage honesty

**Read line-by-line (complete hunk read, upstream + the corresponding rustty code):**
`c092b2bcf`, `b61fd5fbb`, `d5c7e54ae`, `457c5a0a6`, `d44baa914`, `86ec14633`,
`b867a0f59`, `627f34309`, `0fb3565c7` (header+API surface), `68beeeb3f` (Terminal.zig
hunk only — see below), `70e41e96d` (header), `c992658b2`, `d7bb4b863`, `8838c37f4`,
`79aa256fa`, `d4391ff83`, `2ed67cadd` (my three files, complete), `ac4dc1205`,
`5b70f208b`, `33d34cf5c`, `38e891e6c`, `727b8a02f` (my three files, complete),
`4371871bc` (my two files, complete), `bfd40c84b`, `7cd2f65f5`, `e20564791`,
`f973bd53b`, `afb351f83`, `b53728241` (my files), `0aa71d02e`, `a69a591af` (symbol
surface + test names), `87f69a12e` (my two files, complete), `1fdbb8c91` (my files),
`946422dbc` (main.zig; fraction.zig by consumer survey), `0d37f2d34`, `9673a22b0`
(my files, complete), `0e8b7bea6` (my file, complete), `51a4311ef`,
`492c26067` (first 60 lines + symbol survey), `88ed6bebf` (first 100 lines + all
test names), `e3056658d`/`74a233b54` (my-file hunk heads ~90/60 lines).

**Sampled / symbol-level only — stated so it is not mistaken for line coverage:**

- **Bundle A-3's full body** (`68beeeb3f` stream.zig +781 and
  `stream_continuation.zig` +540/+66; `a69a591af` +196): verified ABSENT by symbol
  probes (`continuation`, `until_ground`, `apc_end` signature, file existence) and by
  reading the commit messages + added-symbol inventories + all 24 test names. I did
  **not** read the tracker/boundary-scanner implementations line-by-line; since the
  verdict is ABSENT and porting must go from the head files anyway, line-level reading
  belongs to the porting task, not this vet.
- **A-1/A-2 bodies** (`fc1bd06a1` +289, `35db32078` +331, reflow chain ~1,000):
  same treatment — added-symbol inventories (`grep -oE '[a-zA-Z_]+\('` per commit),
  test-name inventories, head-file reads of `bulkRunLength`/`copyRun`/
  `hyperlinkStringsFit`, and negative rustty greps. Not every interior line.
- **`3d9b2b483`** (+417 over 5 files): read the commit message's four-change summary,
  the Terminal.zig hunk in full, verified each of the four changes absent in rustty
  by targeted grep. PageList/bitmap/hash_map hunk interiors not read line-by-line.
- **Filter statement:** every rustty "still pre-range" claim is anchored to a
  concrete file:line quoted in §2, obtained by reading that rustty code, not by grep
  absence alone, except pure new-symbol absences (Builder, allocatePage, fastprint,
  fraction, stream_continuation, WasmPagePool, grapheme_max_len, eql_any,
  applied_styles, search_start, title_report, Iterator) where grep over
  `crates/rustty-vt/src` (and `crates/` for the pool) is the evidence.
- I did **not** re-verify commits outside my file set that share PRs with mine
  (e.g. `4371871bc`'s graphics_storage rewrite, `727b8a02f`'s osc.zig cap,
  `1fdbb8c91`'s build.zig machinery, snapshot files) beyond confirming file ownership
  via `--stat`.

## 7. Cross-area couplings (name the owner)

| change | also lands in | owner |
|---|---|---|
| A-1 Builder/allocatePage, `b61fd5fbb` iterator, `d44baa914` main re-export, `86ec14633`/`b867a0f59` | `src/terminal/snapshot/*`, `c/snapshot.zig` | snapshot area (+ area C for `c/`) |
| A-3 continuation (`68beeeb3f`, `70e41e96d`, `a69a591af`, `d7bb4b863`) | `c/io.zig` (+755), `include/ghostty/vt/io.h`, `src/lib/TinyIo.zig`, snapshot continuation record | area C + snapshot |
| `b53728241` ApcEnd signature | `apc.zig` (+360, area D — G1b apc drop=4), vt-c dispatch | area D / C |
| A-4 fastprint/PinMap | `formatter.zig` (+1,187, area D — G1b formatter drop=2; also `997a2aff2`, `e523cf810` are **entirely** formatter.zig) | area D |
| `38e891e6c` title_report option | `c/terminal.zig` (+65), `include/ghostty/vt/terminal.h` | area C |
| `4371871bc` infallible setLimit | `kitty/graphics_storage.zig` (−273 rewrite), `c/terminal.zig`, `termio/Termio.zig` | area D / C / E |
| `727b8a02f` OSC 8 MiB cap | `osc.zig` | area D |
| `c092b2bcf` DECECM mode | `modes.zig` (G1b modes drop=1) | area D |
| `7cd2f65f5` color override reset | `color.zig` | area D |
| render trio | `c/render.zig` (+784), `c/style.zig`, `c/types.zig` | area C |
| `9673a22b0`/`0e8b7bea6` ABI metadata call sites | `src/lib/{c_abi,packed}.zig`, `c/types.schema.json`, `c/types-schema-verify.py` | area C (blocking) |
| `492c26067`/`3d9b2b483`(wasm hash)/`87f69a12e`(wasm arms) | `src/datastruct/wasm_page_pool.zig`, `src/lib/allocator/wasm.zig`, simd | datastruct/wasm area (blocking for the wasm arms only; native arms of `3d9b2b483` are unblocked) |
| `1fdbb8c91` vt-features | `src/terminal/build_options.zig` (+254), build.zig | build/config area |
| `946422dbc` fraction consumers | `color.zig`, `apc/glyph*`, `SelectionGesture.zig` | area D (callers); the parser file itself is mine |

## 8. Recommended landing order (area A)

1. **Wave 0 — live-bug/security hotfix (S, no cross-area):** `33d34cf5c`,
   `d5c7e54ae`, `38e891e6c` (Zig-side gate; C option can trail), `727b8a02f` my side,
   `bfd40c84b`, `f973bd53b`, `c092b2bcf`/`7cd2f65f5` test sides (with area D),
   `ac4dc1205`, `c992658b2`, A-5 (`printRepeat`).
2. **Wave 1 — A-2 reflow end-state** (with `d5c7e54ae` already in) + `3d9b2b483`
   native arms. Miri proof for `46276d046`/`copyRun`.
3. **Wave 2 — A-1 Builder/PageAllocation + `b61fd5fbb`** (unblocks snapshot area).
4. **Wave 3 — A-3 continuation** (unblocks snapshot pty-continuation + c/io).
5. **Wave 4 — A-4 fastprint/PinMap** jointly with area D's formatter work.
6. **Wave 5 — render trio** jointly with area C; `74efadb44` XTGETTCAP any time.
7. **Trailing, blocked:** `9673a22b0`/`0e8b7bea6` (on area C lib metadata),
   `492c26067` + wasm arms of `3d9b2b483`/`87f69a12e` (on wasm/datastruct decisions),
   `1fdbb8c91` gates (on build_options flag work).
