# Phase 3 assessment — area D: terminal subsystems

**Range:** `4749c4e93` → `6ad1fe7d8` (365 commits).
**Scope:** every `src/terminal/*` file in the range except `PageList.zig`, `Terminal.zig`,
`Screen.zig`, `page.zig`, `mem.zig` (area A), `compress/*` (area B), `c/*` (area C);
plus `src/unicode/**` and `src/simd/**`.
**Oracle read-only:** all upstream state read via `git -C ../ghostty show origin/main:<path>` /
`git diff`. The oracle tree was never checked out.
**Build state:** `cargo check -p rustty-vt` clean at the worktree HEAD.

---

## 0. Coverage statement — what is exhaustive vs sampled

**Exhaustive (read every hunk of the upstream diff):**

- `hash_map.zig`, `stream.zig`, `render.zig`, `color.zig`, `SelectionGesture.zig`,
  `osc.zig`, `osc/parsers.zig`, `osc/parsers/color.zig`,
  `osc/parsers/kitty_dnd_protocol.zig` (all 363 new lines),
  `search/{Thread,active,pagelist,screen,sliding_window,viewport}.zig`,
  `src/unicode/**` (all 5 files), `src/simd/**` (all 3 files),
  and the whole small-file set: `ScreenSet.zig`, `Selection.zig`, `StringMap.zig`,
  `Tabstops.zig`, `apc.zig`, `apc/glyph/request.zig`, `bitmap_allocator.zig`,
  `clipboard.zig`, `compress.zig`, `dcs.zig`, `device_status.zig`, `hyperlink.zig`,
  `main.zig`, `modes.zig`, `mouse.zig`, `ref_counted_set.zig`, `style.zig`,
  `x11_color.zig`, `tmux/{control,output,viewer}.zig`.
- **Test inventories compared name-by-name** (upstream `grep '^test '` vs rustty
  `#[test]` fn names) for all 6 `search/*` files. That is how the exact
  missing-test set below was established, not by sampling.

**Sampled (all non-test hunks read in full; test hunks spot-checked):**

- `stream_terminal.zig` (+1103/−130): every non-test hunk read; of ~50 test hunks I read
  the 5 new `resize*` tests and the `semantic failure` test in full and confirmed the
  remaining ~45 are `Terminal.init(io, …)` threading only.
- `formatter.zig` (+291/−193): all 6 non-test hunks read; the ~90 test hunks were
  pattern-verified as `const io = testing.io` + `node.page()` + `.data` accessor
  rewrites (no assertion changed).
- `kitty/graphics_{image,storage,exec}.zig`: all semantic hunks read
  (`Usage`/transient, eviction `block` key, `Limits.temporary_file` union,
  `nextGeneration(io)`, `isPathInTempDir(io, dir, path)`, the new
  `kittygfx generation` test); the ~60 remaining test hunks are `io` threading.

**Not verified by execution.** No test was run beyond `cargo check -p rustty-vt`.
Every "PORTED" row below is backed by a rustty `file:line` symbol; where I could not
find a guarding test I say so explicitly rather than assuming one exists.

---

## 1. Summary

| Verdict | Upstream items | Notes |
|---|---|---|
| **ABSENT** | **22** | real missing content, none of it in the ledger's "done" column |
| **PARTIAL** | **4** | ported at an *older upstream shape*; exact missing hunks in §3 |
| **PORTED (evidence-backed)** | **27** | incl. all of BL-7, BL-6, BL-5-in-my-area, and — contrary to `MAIN_PARITY.md` — **most of the BL-1 search stack** |
| **N/A (Zig 0.16 / reflection / cross-area mechanical)** | **15** | itemised in §2.4 |

**Two ledger corrections, in both directions:**

1. **`MAIN_PARITY.md` is stale-pessimistic on the search subsystem.** It lists
   `73ac36fa5`, `86e1f7b8b`, `5d8eb78b7`, `d239f9805`, `83aada205`, `c243d89bd`,
   `a89c6133c`, `89eca063e`, `c9ad708ef`, `f6d9a582e`, `0d015b2fc` as
   `missing BL-1`. **All eleven are in rustty**, landed by WS7, at upstream's final
   shape — `screen_generations` (`search/thread.rs:1017`), `deinit_screen_invalid`
   (`search/screen.rs:316`), `node_is_valid` validation
   (`search/screen.rs:593-603`), `page_serial_epoch` (`search/screen.rs:664`),
   `cross_page_end_rows` (`search/sliding_window.rs:527,709,816`),
   `append_if_wrapped` (`search/sliding_window.rs:912`), `validate_selected_history`
   (`search/screen.rs:610`). rustty's `search/screen.rs` has **22 of upstream's 23
   tests** and `search/pagelist.rs` has **9 of 9** (including the compressed-history
   boundary test). Phase 3 should not re-plan these.
2. **`MAIN_PARITY.md` cannot speak for the 21 commits after its triage head
   `53bd14fec`.** Of those, **12 land real content in my area and every one is
   ABSENT**: `6c8c07981`, `628adaf30`, `47d602c42`, `c3655ba25`, `86f81fb5b`(area-A
   driven), `39ae85f04`, `5caf20e3e`, `a65e11cc9`, `c594031d5`, `439d35e27`,
   `a3c1caba5`, `89b103dd5`, plus the `fedd42e8d`/`65f953e8e`/`7e14347c1` page-map
   trio and the `9659167ec`/`bc8bb6c0f` viewport-fingerprint pair.

**Highest-value findings (details in §3/§4):**

- **A live bug, not just a gap.** `search/sliding_window.rs` lacks upstream's
  `if (self.needle.len == 0) return null` guard in `next()` (`5bc6588e4`), *and*
  rustty's own `ascii_index_of_ignore_case` returns `Some(0)` for an empty needle
  (`sliding_window.rs:1165`). An empty needle therefore produces a zero-length match
  that `highlight()` cannot represent (its end offset is inclusive). Zig's
  `std.ascii.indexOfIgnoreCase` returns `null` there, so **rustty is strictly worse
  than the pin**, not merely behind head.
- **A dangling-node window.** `627518447` extracted `resetIfDimensionsChanged` and
  called it from `reloadActive` *and* `select`, with the comment "This check must
  precede all inspection of cached highlights and searchers: column reflow may have
  freed every node they reference." rustty has the reset **only inline in `feed`**
  (`search/screen.rs:496-533`); `reload_active` (`:822`) and `select` (`:1409`) walk
  cached `FlattenedHighlight` node pointers with no dimension check. A resize between
  feeds lets both paths dereference freed `PageList` nodes.
- **A pointer-stability contract change with cross-area UB consequences.**
  `fedd42e8d` replaced hash-map tombstones with backward-shift deletion, which
  *moves other entries on removal*. rustty's `hash_map.rs` is entirely at the
  tombstone shape (`Metadata::TOMBSTONE` at `:248`, `remove_by_index` at `:885`).
  Porting it invalidates every key/value pointer previously handed out by
  `page.rs`'s hyperlink and grapheme maps — an area-A caller audit is mandatory.
- **A real behavioural divergence in the scalar UTF-8 decoder.** `cb4c49fbf` made a
  valid-so-far sequence cut off by ESC resolve to a single U+FFFD instead of being
  left pending. rustty still unconditionally returns "incomplete"
  (`rustty-simd/src/vt.rs:94-101`), so `"hi\xc2\x1b[0m"` decodes differently from
  upstream.

---

## 2. Per-change verdict table

### 2.1 ABSENT

| upstream | what changed | rustty status | evidence | guarding test/gate | cost | risk |
|---|---|---|---|---|---|---|
| `fedd42e8d` (+`f2a7652ab` touchup) | `hash_map.zig`: tombstones → backward-shift deletion (Knuth 6.4 alg. R); `removeByIndexContext` moves cluster entries; `Metadata` becomes `packed(u8)` with `isFree() == (byte==0)`; `remove()` deleted; forced-inline Wyhash for `hasUniqueRepresentation` keys; 4 new tests + `expectCanonical` invariant helper | **ABSENT** | `crates/rustty-vt/src/hash_map.rs:248` `const TOMBSTONE: u8 = 1`; `:278` `is_tombstone`; `:802-839` `first_tombstone_idx` recycling; `:885` `fn remove_by_index` (single-slot clear); test `HashMap_ensureUnusedCapacity_with_tombstones` at `:1129` | none — needs the 4 upstream tests **plus** `expectCanonical` as a Rust invariant assert | **L** | **High.** Changes pointer-stability contract (§4.5); area-A caller audit required |
| `7e14347c1` | `hash_map.zig`: `max_load_percentage` comptime param on `AutoOffsetHashMap`/`OffsetHashMap`/`HashMapUnmanaged`; `maxLoad()`; `growIfNeeded`→`checkCapacity`; `layoutForSize` (round entries up to the load factor, then to a power of two); `default_max_load_percentage = 100`; `hyperlink.zig` `Map` switched to load **80** | **ABSENT** | `crates/rustty-vt/src/hyperlink.rs:57` `pub type Map = AutoOffsetHashMap<Offset<Cell>, Id>` (no load param); no `max_load`/`layout_for_size` in `hash_map.rs` (`grep -c` = 0) | none — needs `HashMap no-clobber move after remove at max load` + `OffsetHashMap maximum load leaves probe headroom` | **M** | Med. Changes `Page` layout byte size ⇒ area-A `page.rs` capacity math + all size goldens |
| `65f953e8e` | avoid duplicate probes when moving cell data (hash_map half; page.zig half is area A) | **ABSENT** | same as above | none | **S** | Low, but ordered after `fedd42e8d` |
| `446f80f4e` | `render.zig`: two-phase `beginUpdate`/`endUpdate`; `pending_styles: ArrayList(StyleRun)`; per-**page-chunk** iteration replacing per-row `rowIterator` + `last_dirty_page`; `RowBuilder`; `page.Mask`-based group scans (`RowDirtyMask`, `CellSpecialMask`, `scan_group_len = 8`); style-run detection with `pattern`/`bits`/`eql`/`eqlExact`; palette copied only when `redraw`; new `incremental updates match full rebuild` test | **ABSENT** (the `serial` field alone landed — see §3.1) | `crates/rustty-vt/src/render.rs:521` `pub fn update` is the only entry point; `:659` `let mut last_dirty_page`; `:663` `row_iterator`; `:592` unconditional `self.colors.palette = …`; no `begin_update`/`end_update`/`pending_styles`/`RowBuilder` (`grep -c` = 0) | none — needs the upstream differential test | **L** | **High.** Aliasing-heavy (§4.1). Depends on area-A `page.Mask` (absent: upstream `page.zig:2303`, no rustty counterpart) |
| `39ae85f04` | `dcs.zig`: `DECRQSS.max_response_bytes = 256` + `DECRQSS.encode(t, buf)` producing `ESC P {0\|1} $ r … ESC \`; `stream_terminal.zig` `dcs_handler` field + `dcsHook`/`dcsPut`/`dcsUnhook`/`dcsCommand`; 2 tests | **ABSENT** | `crates/rustty-vt/src/dcs.rs` has `Handler` (`:69`) and `Command::Decrqss` (`:280-286`) but no `encode`/`max_response_bytes`; `stream_terminal.rs` has no `dcs_handler` (`grep -c` = 0) — DCS actions are still the pin's no-ops | none — needs `DECRQSS response encoding` + `DECRQSS largest response fits fixed buffer` | **M** | Med. Needs `Terminal::print_attributes` + `set_cursor_style`; the latter is gated on `c594031d5` |
| `6c8c07981` | visibility reports: `device_status.zig` `Visibility` enum + `encodeVisibilityReport` + `max_visibility_report_encode_size` + DSR entry `998` (question); `modes.zig` `report_visibility = 2033`; `stream_terminal.zig` `sendVisibilityReport` + `.visibility` DSR arm + mode-2033 enable arm | **ABSENT** | `grep -n "Visibility\|998\|report_visibility" crates/rustty-vt/src/{device_status,modes}.rs` → **0 hits**; `modes.rs:229-230,477-478` end at `2031`/`2048` | none — needs `encode visibility report` + the stream_terminal DSR/mode tests | **M** | Med. Needs area-A `Terminal.flags.visible` (`grep visible crates/rustty-vt/src/terminal.rs` → no such flag) and area-E `termio` plumbing |
| `47d602c42` | `stream_terminal.zig`: `Effects.progress_report` callback + `progressReport()` dispatch + `readonly` entry | **ABSENT** | `crates/rustty-vt/src/stream_terminal.rs:1921` `fn progress_report(&mut self, v: osc::ProgressReport) {}` — a deliberate no-op stub matching the pin | none | **S** | Low; area-C header/`c/terminal.zig` half is the bulk |
| `c3655ba25` | `stream_terminal.zig`: `Effects.desktop_notification` callback + `desktopNotification()` dispatch + `readonly` entry | **ABSENT** | `crates/rustty-vt/src/stream_terminal.rs:1918` `fn show_desktop_notification(&mut self, v: stream::ShowDesktopNotification) {}` — no-op stub | none | **S** | Low; area-C header half is the bulk |
| `439d35e27` | `stream_terminal.zig`: `semantic_failure: bool` sticky flag set in `vt()`'s catch; `setAttribute`/`windowTitle`/`reportPwd` promoted from swallow-and-log to `try`; `colorOperation`/`kittyColorOperation`/`clipboardContents` stay logged (graceful) | **ABSENT** | `grep -n semantic_failure crates/rustty-vt/src/stream_terminal.rs` → **0 hits** | none — needs `semantic failure is sticky while processing continues` | **S** | Low, but it re-partitions which errors are "handled"; must be ported *with* the try/log split, not just the flag |
| `c594031d5` | move DECSCUSR defaults out of the handler into terminal state: `Handler.default_cursor{,_style,_blink}` deleted; `.cursor_style` → `terminal.setCursorStyle(value)`; `.full_reset` → plain `terminal.fullReset()` | **ABSENT** | `crates/rustty-vt/src/stream_terminal.rs:108` `pub default_cursor_style: CursorStyle`, `:227` init, `:1522-1531` + `:1674-1681` the handler-side style/blink logic still present | none | **M** | Med. Area-A `Terminal.zig` (+144) owns the receiving side; also area-E `stream_handler` (−68) |
| `89b103dd5` | `stream_terminal.zig`: `pub fn resize(self, value: Terminal.Resize)` — resizes then, when `cell_size_px` is present *and* mode 2048 is on *and* `write_pty` exists, emits an in-band `size_report.encode(.mode_2048, …)`; 4 new tests | **ABSENT** | `crates/rustty-vt/src/terminal.rs:4556` is still pin-era `resize(&mut self, alloc, cols, rows)`; no `Resize` struct, no `cell_size_px`, no `Handler::resize` | none — needs all 4 upstream `resize*` tests | **L** | **High.** Blocked on area-A `Terminal.Resize` (+193 in `Terminal.zig`) |
| `a3c1caba5` | resize failures almost fully safe (`stream_terminal.zig` half: `resize failure preserves terminal state and does not write`, `resize effects do not change canonical terminal state`) | **ABSENT** | as above | none | **M** | Med; strictly after `89b103dd5` |
| `5caf20e3e` | `Tabstops.resize`: `if (size < self.dynamic_stops.len)` → `<=`, so a resize to the exact existing capacity no longer reallocates | **ABSENT** | `crates/rustty-vt/src/tabstops.rs:252` still `if size < self.dynamic_stops.len()` | none — needs `Tabstops: resize to existing capacity does not allocate` (a fixed-buffer-allocator test; needs a rustty `FixedBufferAllocator` equivalent) | **S** | Low |
| `a65e11cc9` | kitty transient usage hints: `Transmission.usage: Usage` (`packed(u32)`, `transient: bool`) parsed from the `N` key; `Image.usage`; eviction sorts on a `block: u2` key `(transient?0:1) + (used?2:0)` so transient/unused images evict first; `Image` API `nextGeneration` re-export; 1 new parser test | **ABSENT** | `grep -rn "transient\|usage\|Usage" crates/rustty-vt/src/kitty/graphics_command.rs` → **0 hits**; `graphics_storage.rs:921` `struct Candidate { id, generation, used }` — no `block` | none — needs `transmission command with transient hint` + an eviction-order test (upstream has none for the ordering; **write one**) | **M** | Med. Behaviour-visible eviction change |
| `e8525c0fd` (kitty subset) | `LoadingImage.Limits.temporary_file` becomes a tagged union carrying the expected temp **directory**; `Limits.all` → `Limits.allWithTempDir(path)`; `LoadingImage.temporary_directory` field; `isPathInTempDir(io, dir, path)` takes the directory explicitly instead of consulting the process env | **ABSENT** | `crates/rustty-vt/src/kitty/graphics_image.rs:128` `pub temporary_file: bool`, `:133` `pub const ALL`, `:526` `fn isPathInTempDir(path: &[u8])` reading `temp_dir::allocTmpDir()` at `:536` | none | **M** | **Med-high security relevance:** upstream removed a global-env dependency from a path-validation check. Ripples to area-C (`c/*`, `include/`) and area-E (`os`) |
| `f6f79acce` | dispatch APC string bytes in bulk: `apc.zig` `Handler.feedSlice`; `apc/glyph/request.zig` `CommandParser.feedSlice` (+`error.ReadFailed` arm); `kitty/graphics_command.zig` `Parser.feedSlice`; `stream.zig` `Action.ApcPutSlice` + `apc_put_slice` + `consumeApcString`; `stream_terminal.zig` dispatch arm; 5 apc + 4 kitty + 6 stream tests | **ABSENT** | `grep -rn feed_slice crates/rustty-vt/src/` → **0 hits**; `apc/mod.rs:103` byte-at-a-time `feed` only | none — needs all 15 upstream tests, esp. `stream: apc vector boundaries match scalar path` | **L** | **High.** Aliasing hazard in `apc.Handler.feedSlice` (§4.6) |
| `8c523ed03` | vectorize APC payload scanning (the SIMD `invalid`-mask loop inside `consumeApcString`) | **ABSENT** | as above | none | **S** | Low; strictly after `f6f79acce` |
| `47e26df60` | batch printed codepoint runs: `stream.zig` `Action.PrintSlice` + `print_slice` + the lane-wise printable-run scan in `nextSlice`; `simd/main.zig` `pub fn lanes(T)` (= 2× native vector width); `stream_terminal.zig` `.print_slice => terminal.printSlice(value.cps)` | **ABSENT** | `grep -n "print_slice\|PrintSlice" crates/rustty-vt/src/stream.rs` → **0 hits**; no `fn lanes` in `crates/rustty-simd/src/lib.rs` | none | **L** | **High.** Needs area-A `Terminal::print_slice`/`printSliceFill` (`cb2d78587`, also absent) |
| `253e4f9c3` | `stream.zig` `consumeCsiParams` — bulk-consume CSI digits/separators and dispatch on the final byte | **ABSENT** | `grep -n consume_csi_params crates/rustty-vt/src/stream.rs` → 0 | none — upstream ships tests with it | **M** | Med. Aliasing (§4.7): `p = &mut self.parser` then `self.csiDispatchFinal(c)` |
| `1a88f3622` + `300b42f42a` | `stream.zig` `csiDispatchFinal` (dispatch a CSI straight out of parser state), `csiEntryByte` fast path, and the `@hasDecl(T,"vtRaw")` gate that keeps the inspector on the general path | **ABSENT** | `grep -n "csi_dispatch_final\|csi_entry_byte" crates/rustty-vt/src/stream.rs` → 0 | none | **M** | Med. Must reproduce the `vtRaw` gate or the inspector (area F) silently loses raw events |
| `b5053153f` | `stream.zig` `logUnsupportedOnce(fmt, args, key)` — 16-slot atomic `u32` per-call-site dedup table; 8 `log.warn` sites converted | **ABSENT** | `grep -n log_unsupported_once crates/rustty-vt/src/stream.rs` → 0 | none | **S** | Low |
| `083d9709b` | `simd/vt.cpp`: ASCII fast path fused into the ESC scan — `WidenAsciiStore` (byte→u32 widening via `PromoteTo` on quarter vectors, with an `HWY_TARGET == HWY_SCALAR` arm) and `DecodeNonAsciiUntilControlSeq(d, input, count, base, …)` as the non-ASCII continuation | **ABSENT** | `crates/rustty-simd/cbits/simd/vt.cpp` — `grep -n "WidenAsciiStore\|DecodeNonAsciiUntilControlSeq\|ASCII fast path"` → 0 | none — needs the new `decode simd matches scalar` differential test | **M** | Med. C++ vendored file; needs the `HWY_SCALAR` arm or wasm/scalar targets break |
| `cb4c49fbf` | `simd/vt.zig` scalar decoder: a valid-so-far multi-byte sequence truncated by the **decode region** is only left pending when the region ends at the true end of input; when the region was bounded by an ESC it can never complete, so the prefix is consumed as one maximal subpart → one U+FFFD | **ABSENT** | `crates/rustty-simd/src/vt.rs:94-101` returns `DecodeResult { consumed: decode_offset, decoded: decode_count }` unconditionally — no `decode.len() == input.len()` guard, no `break` | none — needs `decode partial UTF-8 before escape` (2 cases) | **S** | **Med-high: this is a behavioural divergence, not perf.** `"hi\xc2\x1b[0m"` and `"\xe0\xa0\x1bX"` decode differently from upstream |

### 2.2 PARTIAL — see §3 for the exact missing hunks

| upstream | rustty status | evidence |
|---|---|---|
| `627518447` search: reset cached results after resize | **PARTIAL** | inline reset present in `search/screen.rs:496-533` (`feed` only); no `reset_if_dimensions_changed`; absent from `reload_active` (`:822`) and `select` (`:1409`); upstream test `select after resize resets stale flattened results` is the **one** missing test of 23 |
| `5bc6588e4` search: ignore empty search needles | **PARTIAL** | all three `needle.len() - 1` sites unsaturated (`search/viewport.rs:262,315`, `search/active.rs:132`); both `needle.len == 0` early-returns absent (`sliding_window.rs:286`, `active.rs:74`); the missing test is `SlidingWindow empty needle has no matches` (25 of 26 present) |
| `9659167ec` + `bc8bb6c0f` search: reuse / don't clear viewport fingerprint storage | **PARTIAL** | rustty is at the `c243d89bd` shape: `fingerprint: Option<Fingerprint>` with `init`/`eql`/`deinit` (`search/viewport.rs:41,123-197`). Missing: non-optional `Fingerprint = .{}`, `clear_and_retain_capacity`, and the single-pass `update(alloc, pages) -> bool`. Missing test: `fingerprint only allocates on changes` (5 of 6 present) |
| `446f80f4e` render state update optimizations | **PARTIAL** in the narrow sense that its `serial` field landed via `c9ad708ef` (`render.rs:196,705`); **everything else ABSENT** — counted as ABSENT above |

### 2.3 PORTED — evidence-backed

| upstream | rustty evidence | guarding test |
|---|---|---|
| `91c87e2cf` + `dd6c09bcf` + `a6132c18b` + `c488ccda6` (BL-7, OSC 72) | `crates/rustty-vt/src/osc/parsers/kitty_dnd_protocol.rs` (530 lines); `OSC.metadata: Vec<u8>` / `payload: Option<Vec<u8>>` (`:31,35` — raw bytes, **no** lossy `String`); `EventType::init` all 13 verbs (`:92-104`); case-sensitive `read` (`:196`); `parse` (`:280`); `osc/mod.rs:553` `S72`, `:1031` `b'2' => State::S72` | 12 upstream tests all present in the module's `mod tests` |
| `bdc0b6c19` kitty/gfx generation stamps | `kitty/graphics_storage.rs:65` `next_generation`, `:111` `generation`, `:205` `mark_mutated`; `graphics_image.rs:773` `Image.generation` | `storage_generation_stamps_on_image_add_and_replace` (`graphics_storage.rs:2645`) |
| `2970e9a2a` lib-vt color utility APIs | `color.rs:98` `PaletteEntry`, `:114` `parse_palette_entry`, `:836/:844` `encode_rgb8/16`, `:959` `RGB::parse` with `trim_matches(' '\|'\t')`, `:656/:671` the no-`#` 3-/6-digit arms | `color.rs` tests incl. the `0A0B0C`/`FFF`/`\tForestGreen\t` cases |
| `2970e9a2a` (x11 half) | `x11_color.rs:99` `Entry`, `:108` `entries()`, NUL-terminated names | `x11_entries_order_and_map_parity` (`:193`) |
| `f00e90694` color scheme report encoder | `device_status.rs:118` `encode_color_scheme_report`, `:111` max-size const | `encode_color_scheme_report_dark_light` (`:175`) |
| `fc5a72772` codepoint width API | `rustty-unicode/src/lib.rs:44` `codepoint_width` | `codepoint_width_table` (`:54`), all 20 upstream assertions |
| `65e61282a` grapheme width API | `rustty-unicode/src/grapheme.rs:59` `GraphemeWidthEffect`, `:77` `GraphemeWidth`, `:98` `grapheme_width_effect`, `:127` `invalid_codepoint`, `:145` `grapheme_width` | the 5 upstream `grapheme width: *` tests |
| `14c829883` report OSC color queries | `stream_terminal.rs:664` `color_for_xterm`, `:693` `write_xterm_color_report`, `:823` `color_for_kitty`; `colorOperation(requests, terminator)` signature matches head | KATs noted in `MAIN_PARITY.md` §3 |
| `14c829883` (kitty half) | `kitty/color.rs:116` `has_terminal_query_color` | `:158-160` |
| `634ef7198` protocol-neutral clipboard writes | `crates/rustty-vt/src/clipboard.rs` (Location/Content/Write/WriteResult); `stream_terminal.rs:143` `clipboard_write` effect, `:1938` dispatch | `clipboard_write_effect_callback` (`:3148`) |
| `3a2e28329` scroll-to-row | `terminal.rs:318` `ScrollViewport` with a `Row` variant | (per `MAIN_PARITY.md` §3; not re-verified) |
| `b953bb346` bitmap allocator chunk sizing | `bitmap_allocator.rs:335` | 2 named tests |
| `e44f5cb0f` RefCountedSet zero-capacity guard | `ref_counted_set.rs:825` | `style.rs` `set_zero_capacity` |
| `3d08161b5` empty tabstop ranges | `tabstops.rs:315` | `tabstops_interval_with_zero_columns` |
| `8f1c2fe95` backwards selection timestamps | `selection_gesture.rs:790` `fn time_since(..) -> Option<u64>` | `backwards_repeat_time_resets_click_count` (`:2435`) |
| `b6f34be44` / `a9f5b7eba` / `08e376f23` selection clamps + drag geometry | `selection_gesture.rs:829` `columns == 0 \|\| cell_width == 0`, `:849-853` `checked_mul` pixel span; `selection.rs` mirrored/`contained_row_cached` arms | per §1/§3 of `MAIN_PARITY.md`; the two **new** upstream mixed-width tests are missing (see §3.5) |
| `73ac36fa5` discard destroyed screen state | `search/thread.rs:1017` `screen_generations`, `:1219` generation check, `:1188/:1381` `deinit_screen_invalid`; `search/screen.rs:316` `deinit_screen_invalid`, `:338` `deinit_list_invalid`; `search/pagelist.rs:150` `deinit_list_invalid` | thread.rs has both upstream tests incl. `select after active screen removal` |
| `86e1f7b8b` reject replaced result pages | `search/screen.rs:593-603` `history_result_is_valid` → `node_is_valid(node, serial)` | `select_after_history_compaction_ignores_replaced_results` (`:2788`) |
| `5d8eb78b7` reset pins while feeding | `search/pagelist.rs:216-219` sets `pin.node/.y/.x/.garbage` | `feed_keeps_its_tracked_pin_within_a_shorter_page` (`:847`) |
| `d239f9805` drop pruned selections | `search/screen.rs:663-720` prune loop with per-result selection fixups | `select_after_partial_history_erase_drops_a_pruned_selection` (`:2742`) |
| `83aada205` preserve serial order in reverse matches | `search/sliding_window.rs:527,709,816` `cross_page_end_rows`; `Meta.rows` snapshot | reverse-match tests present (26-test inventory minus the empty-needle one) |
| `c243d89bd` generations in viewport fingerprints | `search/viewport.rs` `Fingerprint::Entry { node, serial }` | `fingerprint_detects_node_generation_changes` (`:530`) |
| `a89c6133c` validate history before reload | `search/screen.rs:610` `validate_selected_history`, called first in `reload_active` (`:831`) | `reload_defers_pruning_unselected_history_results` (`:2836`) |
| `89eca063e` avoid pruning history on reload | `reload_active` does **not** prune; `select` (`:1409`) does `reload_active()` then `prune_history()` — matches head | `reload_after_partial_history_page_erase_drops_shifted_selection_first` (`:2898`) |
| `f6d9a582e` page serial floor → epoch | `search/screen.rs:664` `page_serial_epoch` | covered by the prune tests |
| `0d015b2fc` preserve compressed pages during search | `search/sliding_window.rs:886` `append`, `:912` `append_if_wrapped` via preserved pages; `search/active.rs:128` | `compressed_history_match_spanning_page_boundary_remains_compressed` (`search/pagelist.rs:615`) |
| `8fe5913f7` no-64-bit-atomics fallback | **N/A with evidence** — every rustty target reports `target_has_atomic="64"`; recorded as a module PORT NOTE (`MAIN_PARITY.md` §2 BL-6) | n/a |

### 2.4 N/A — Zig-0.16 / reflection / cross-area mechanical (15)

`e8525c0fd` / `b988efcfe` / `f2a7652ab` mechanical ripple, no semantic content in my area:

- `ScreenSet.zig`, `StringMap.zig`, `tmux/viewer.zig`, `graphics_unicode.zig`,
  `search/*` tests, `render.zig` tests, `stream_terminal.zig` tests, `Selection.zig`
  tests: `std.Io` threading (`init(io, alloc, …)`, `testing.io`) — Rust has no `std.Io`.
- `modes.zig`, `device_status.zig`, `tmux/output.zig`: `@Type(.{…})` → `@Struct`/`@Enum`
  builtins. Rust uses plain enums/structs.
- `osc.zig`, `osc/parsers/color.zig`, `color.zig`: `std.meta.intToEnum` →
  `std.enums.fromInt`. Rust already returns `Option`.
- `tmux/control.zig`: `trimRight` → `trimEnd`.
- `formatter.zig`: `std.io.fixedBufferStream` → `std.Io.Writer.fixed`; `.data` field
  accessors on `page.Cell.content` (area-A `page.zig` packed-struct change);
  `&node.data` → `node.page()` (area-A/B `PageList` storage change).
- `style.zig`: `content.color_palette` → `.color_palette.data` (same area-A change).
- `mouse.zig`: `std.testing.refAllDeclsRecursive` → `lib/compat/testing`.
- `unicode/props.zig`, `unicode/grapheme.zig` `Precompute.Value`: explicit `_padding`
  to reach a power-of-two backing integer — a Zig/LLVM exotic-width workaround
  (ziglang/zig#21891). Rust `Properties` (`rustty-unicode/src/props.rs:41`) is a plain
  struct; nothing to do.
- `unicode/props_uucode.zig`, `symbols_uucode.zig`: `main(init: std.process.Init)`
  generator entry points. rustty ships checked-in tables + a `checksum.rs`; N/A.
- `osc.zig` `.color_operation` moved into the deinit'd arm (`v.requests.deinit`):
  fixes a Zig leak. `osc/mod.rs:722` `Command::ColorOperation { .. } => {}` is correct
  in Rust — `Drop` frees the storage.
- `osc/parsers/color.zig` `std.SegmentedList` → the new
  `src/datastruct/segmented_list.zig`: rustty models the storage directly
  (`osc/parsers/color.rs:417-419` PORT NOTE). Cross-area note for `src/datastruct`.
- `628adaf30` share progress state enum: `ProgressReport.State` becomes a
  `LibEnum` and the `ghostty.h` conformance test moves out of the enum body —
  Zig-reflection only in `osc.zig`; the `c/terminal.zig` half is area C.
- `86f81fb5b` in my files: `max_scrollback` → `max_scrollback_bytes` in ~20 test
  call sites (`SelectionGesture.zig`, `render.zig`, `search/screen.zig`,
  `search/sliding_window.zig`, `Selection.zig`, `StringMap.zig`). The semantic change
  (line-based scrollback limit) is entirely in area-A `Screen.zig`/`Terminal.zig`.
- `659a60ae5` search: fix tests — `PageList.init(alloc, 80, 24, 0)` →
  `PageList.init(alloc, .{…})`, following the area-A signature change.
- `compress.zig` (new namespace) and `main.zig`'s `compression_enabled` /
  `_ = @import("compress.zig")` / `_ = @import("mem.zig")`: areas A/B.
- `apc.zig`'s `.ignore`/`.identify` state names and `max_bytes`: pin-era, unchanged.

---

## 3. Partials and divergences — exact missing hunks

### 3.1 `render.zig` — the `serial` field landed, the restructure did not

`c9ad708ef` (validate flattened highlight generations) is fully in:
`render.rs:196` `pub serial: u64`, `:705` `self.row_data[y].serial = (*row_pin).serial`,
`:1065-1075` the `chunk.serial != row_serial` skip, and the guarding test
`flattened_highlights_require_matching_page_serial` (`:2086`).

`446f80f4e` is not. Missing, hunk by hunk:

1. `RenderState.pending_styles: Vec<StyleRun>` + `StyleRun { y, start, end, style }`
   + `deinit` teardown.
2. `update` split into `begin_update` + `end_update`; `update` becomes
   `begin_update(); end_update();`.
3. `if (redraw) self.pending_styles.clearRetainingCapacity();`
4. Per-row `rowIterator` → `viewport_pin.pageIterator(.right_down, null)` chunk loop
   with `take = min(chunk.end - chunk.start, self.rows - y)`; cursor detection hoisted
   to a per-chunk bounds check; page `dirty` consumed once per chunk;
   `last_dirty_page` bookkeeping (`render.rs:659,718-728,1022`) **deleted**.
5. The pin/serial write-skip: only rewrite `row_pins[y..][0..take]` when the first
   and last entry of the chunk range disagree with `(node, chunk.start±, serial)`.
6. `RowBuilder` struct + `RowBuilder.row(p, page_row, vy)` carrying the per-row rebuild
   out of `update`.
7. `scan_group_len = 8`, `RowDirtyMask = page.Mask(page.Row, &.{"dirty"}, 8)`,
   `CellSpecialMask = page.Mask(page.Cell, &.{"content_tag","style_id"}, 8)` and the
   group-scan loops (`match`/`matchScalar`/`pattern`/`bits`/`eql`/`eqlScalar`/
   `eqlExact`). **`page.Mask` does not exist in rustty** — upstream `page.zig:2303`,
   `grep -n 'Mask' crates/rustty-vt/src/page{,_impl}.rs` → 0. Area-A blocker.
8. `if (cells.len != b.cols) try cells.resize(...)` (skip the resize when unchanged).
9. Style-run detection: one `styles.get` per run, pushed as a `StyleRun`, instead of
   a per-cell `cells_style[x] = …` write.
10. `end_update` denormalizing runs with the defensive `run.y >= row_cells.len` /
    `min(run.end, styles.len)` clamps.
11. Palette copied only under `redraw` (`render.rs:592` is unconditional today).
12. The load-bearing new test `incremental updates match full rebuild`.

### 3.2 `search/screen.zig` — `resetIfDimensionsChanged` (`627518447`)

Present today: the reset body, inline in `feed` (`search/screen.rs:496-533`), with a
correct PORT NOTE about copying the needle to an owned `Vec` before
`self.deinit(); *self = new;` (the aliasing fix).

Missing:

1. Extract it as `fn reset_if_dimensions_changed(&mut self) -> Result<bool, OutOfMemory>`
   returning `true` when the reset happened.
2. `feed` becomes `let _ = self.reset_if_dimensions_changed()?;` with the amended
   comment ("Resize/reflow invalidates every flattened result…").
3. **`reload_active`: `if self.reset_if_dimensions_changed()? { return Ok(()); }` as the
   very first statement**, *before* `validate_selected_history()`. rustty's own
   `reload_active` PORT NOTE at `:826-830` already flags that upstream has this call and
   rustty does not — that note is the smoking gun.
4. **`select`: `let _ = self.reset_if_dimensions_changed()?;` before `reload_active()`**,
   with the comment "A resize can replace any page node, including nodes retained by
   flattened history results."
5. The test `select after resize resets stale flattened results`
   (upstream `search/screen.zig:1204`).

Until (3)/(4) land, both paths dereference cached `FlattenedHighlight` node pointers
that column reflow may have freed. This is the single highest-severity gap in my area.

### 3.3 `search/{sliding_window,active,viewport}.zig` — empty needles (`5bc6588e4`)

1. `sliding_window.rs` `next()` (`:286`): add
   `if self.needle.is_empty() { return None; }` as the first statement. **Required for
   soundness in rustty specifically**, because `ascii_index_of_ignore_case`
   (`:1165`) returns `Some(0)` for an empty needle where Zig's
   `std.ascii.indexOfIgnoreCase` returns `null`.
2. `active.rs` `update()` (`:74`): after `self.window.clear_and_retain_capacity()`,
   add `if self.window.needle.is_empty() { return Ok(None); }`.
3. Saturating overlap: `viewport.rs:262`, `viewport.rs:315`, `active.rs:132` —
   `needle.len() - 1` → `needle.len().saturating_sub(1)`. Upstream hoists it to a
   single `overlap_len` local in `viewport.zig`; do the same.
4. Test `SlidingWindow empty needle has no matches`.

### 3.4 `search/viewport.zig` — fingerprint storage (`9659167ec`, `bc8bb6c0f`)

1. `fingerprint: Option<Fingerprint>` → `fingerprint: Fingerprint` (default-empty).
   Delete every `Option` unwrap at `viewport.rs:77,89,93,123-197`.
2. `Fingerprint::clear_and_retain_capacity()`; `reset()` calls it instead of
   dropping the fingerprint.
3. Replace `init` + `eql` with
   `update(&mut self, alloc, pages) -> Result<bool, OutOfMemory>`:
   `ensure_total_capacity(pages.rows)` first (so an allocation failure preserves the old
   fingerprint), then one pass over `pages.page_iterator(.right_down, .viewport, None)`
   overwriting in place, tracking `changed`, then `shrink_retaining_capacity(i)` and
   returning `changed || i != old_len`.
4. `update()`'s call site inverts: `if !fingerprint.update(...)? { 'match: { … } }`.
5. Iterate `fingerprint.entries` instead of `fingerprint.nodes`; keep the
   `end_append.last_row_wrapped` trailing-overlap form (rustty already has
   `AppendResult`).
6. Test `fingerprint only allocates on changes`.

### 3.5 `Selection.zig` — clamps ported, the two new tests are not

The clamping hunks (`@min(…, p.node.cols() - 1)` in `topLeft`/`bottomRight`/
`containedRowCached`, `_ = s`, `copy.node.cols() - 1` replacing `s.pages.cols - 1`)
are ported per `MAIN_PARITY.md` §1/§3. The two **new** upstream tests are not present:
`Selection: rectangle corners clamp across mixed-width pages` and
`Selection: containedRow clamps mixed-width pages`. Both build a mixed-width page list
via `pages.split(...)` + `second.page().size.cols = 2` and are exactly the
regression arming this row needs; port them with the area-A `split`/`page()` surface.
The `next.node.data.getCells` → `next.node.page().getCells` rewrites in `adjust` are
area-A mechanical.

### 3.6 `kitty/graphics_storage.zig` — eviction ordering

rustty's `Candidate` (`graphics_storage.rs:921`) is `{ id, generation, used }` and the
comparator (`:836-857`) is `used` → `generation` → `id`. Upstream head is
`{ id, generation, used, block }` where `block: u2 = (transient?0:1) + (used?2:0)`, and
the comparator keys on `block` first. Add `Transmission.usage: Usage`
(`packed(u32)`, bit 0 `transient`), parse it from the `N` key in
`Transmission.parse`, carry it onto `Image.usage`, and re-key the sort. Upstream ships
no eviction-order test; **write one** (transient+unused evicts before
non-transient+unused before transient+used).

---

## 4. Aliasing / Stacked-Borrows section

Context: rustty's vt search subsystem already had a real 9-test SB UB cluster, fixed by
restoring raw-pointer semantics — search objects hold raw aliases derived from
`*mut Screen` (notably `PageListSearch.list: *mut PageList`, which points *into* the
`Screen` allocation), so any whole-`Screen` `&mut` retag pops them off the borrow stack.
Every hunk below is one where a naive `&mut` translation reintroduces that class of bug.

### 4.1 `446f80f4e` `RowBuilder` + chunk loop — the worst offender

Upstream's `RowBuilder` holds **seven simultaneous mutable column slices** of the
`row_data` MultiArrayList plus `pending_styles: *ArrayList(StyleRun)`, and its
`row(p, page_row, vy)` receives `p: *page.Page` while the caller is *concurrently*
iterating `page_rows: []page.Row` — a `&mut` slice **into the very same `Page`
allocation** (`p.rows.ptr(p.memory)[chunk.start..][0..take]`), whose `dirty` bits it
clears in the loop.

- Naive port → `&mut Page` + `&mut [Row]` derived from it, live at once, with `&mut Page`
  re-read across the call. That is exactly the
  `rust-mut-args-strongly-protected` pattern: SB UB.
- **Recommended shape:** `RowBuilder { rows: *mut RowData, len: usize, alloc, cols,
  pending_styles: *mut Vec<StyleRun> }`, with `fn row(&self, p: *mut Page,
  page_row: *const Row, vy: usize)`. Never form `&mut Page`; reach cells via
  `(*page_row).cells.ptr((*p).memory)` raw. Reach `row_data[vy]` through
  `&mut *self.rows.add(vy)` created *inside* the narrowest scope and dropped before the
  next raw access. Clear `dirty` through `*mut Row`, not `&mut Row`.
- rustty's `row_data` is a `Vec<RowData>` of structs (not column arrays), which is
  *better*: one raw base pointer replaces seven slices. Do not "improve" it into
  `split_at_mut` gymnastics.
- `page.Mask` (area A) must likewise take `*const [Cell]`/`*const [Row]` bases, not
  `&[Cell]`, if it is ever called while a `*mut Page` alias is live.
- **Prove on Miri** on an isolated harness (the `.port/phase-2/mirihar-chunk3b`
  precedent) before wiring into the FFI build; hand-reasoning SB here has failed before.

### 4.2 `627518447` `resetIfDimensionsChanged` in `reload_active` / `select`

The reset does `self.deinit(); *self = new;` while `self.screen: *mut Screen` is a live
raw alias, and `ScreenSearch::init` borrows `self.needle()`.

- rustty's existing `feed` copy already handles this (`needle_owned: Vec<u8>` first).
  **Reuse that shape verbatim** in the extracted function; do not let
  `ScreenSearch::init(self.allocator(), self.screen, self.needle())` survive the
  refactor — `self.needle()` is a borrow of `self` that must not be live across
  `self.deinit()`.
- In `select`, the new call must precede `reload_active()` and must not be inside any
  borrow of `self.history_results` / `self.active_results` / `self.selected` — after a
  reset those `Vec`s are new allocations, so any surviving reference is dangling.
- `reload_active`'s early `return Ok(())` on a `true` result is load-bearing: falling
  through re-inspects the freshly-replaced state.

### 4.3 `9659167ec` `Fingerprint.update(alloc, pages)` + the entries loop

Upstream snapshots `const entries = fingerprint.entries.items` and then, in the same
scope, calls `self.window.appendIfWrapped(node)` / `self.window.append(entry.node)` —
which mutate the `SlidingWindow` and can reallocate its buffers.

- `&self.fingerprint.entries` + `&mut self.window` are disjoint fields, so plain
  field borrows compile. But `ViewportSearch` reaches the `PageList` through a raw
  alias, and `update(&mut self.fingerprint, pages: *mut PageList)` must **not** be
  called via a `&mut self` method that also holds `&mut self.window` — split it:
  `Fingerprint::update` takes `&mut self` (the fingerprint) and a `*mut PageList`.
- If any future refactor puts `fingerprint` behind a raw pointer, switch the
  entries loop to `for i in 0..self.fingerprint.entries.len()` re-indexing each
  iteration, so no slice reference spans an `append`.
- `ensure_total_capacity` **before** any mutation is required for the
  "allocation failure preserves the old fingerprint" contract — do not reorder.

### 4.4 `sliding_window.appendPage(node, page, last_row_wrapped)`

Already ported, but every new hunk touching it must respect its invariant, which
upstream states explicitly: *"No pointer into `page` may escape this function:
compressed preserved pages own temporary decode storage, while resident values borrow
node memory."* `page` may point into `preserved`'s scratch buffer **or** into the
node — so no `&mut Node`/`&mut Page` may be formed while `page` is live, and
`Meta.rows` must be a **copy** (`page.size.rows`), never a re-read of
`node.data.size.rows` (which is what `83aada205` fixed).

### 4.5 `fedd42e8d` backward-shift deletion — a *contract* aliasing hazard

`removeByIndexContext` writes `metadata[hole] = metadata[j]`, `keys_ptr[hole] =
keys_ptr[j]`, `values_ptr[hole] = values_ptr[j]` — three overlapping raw views into one
`Page` allocation, plus `ctx.hash(keys_ptr[j])` reading a key while holes move.

- Keep `metadata` / `keys()` / `values()` as **raw pointers** (rustty already does:
  `metadata: Option<*mut Metadata>`). Do not form `&mut [K]` and `&mut [Metadata]`
  simultaneously from the same `*mut u8` base for the convenience of indexing.
- **The bigger hazard is the documented contract change:** *"insertion never moves
  existing entries, but removal may move other entries within a probe cluster. Any key
  or value pointers previously returned by the map must be considered invalidated by
  any removal."* rustty's `page.rs` hyperlink map and grapheme map (area A) hand out
  `getPtr`-style results. **Audit every caller in `page.rs`/`page_impl.rs` that keeps a
  key/value pointer across a `remove`/`fetch_remove` before landing this.** In Zig
  that is a logic bug; in Rust with raw pointers it is UB.
- `putAssumeCapacityNoClobberContext`'s new `assert(self.header().size < self.capacity())`
  and `getOrPut`'s new `assert(metadata[0].isFree())` must both be ported as real
  `debug_assert!`s — they are the guards that turn a violated capacity contract into a
  panic instead of a silent live-entry overwrite.

### 4.6 `f6f79acce` `apc.Handler.feedSlice` — borrow of a union payload while reassigning the union

```zig
.kitty => |*p| { p.feedSlice(rem) catch { p.deinit(); self.state = .ignore; }; return; }
```

`p` points **into** `self.state`; the error arm calls `p.deinit()` and then assigns
`self.state`. In Rust `match &mut self.state { State::Kitty(p) => { … self.state = … } }`
does not compile, and "fixing" it by taking a raw pointer to the payload and then
writing the discriminant through `&mut self.state` is an aliasing violation of the same
kind.

- **Recommended shape:** `let mut st = core::mem::replace(&mut self.state, State::Ignore);`
  operate on the owned `st`, then write the final state back — or, matching the
  reference's control flow more literally, take `let p: *mut KittyParser = …` and
  perform `(*p).deinit()` **before** any `&mut self.state` is created, with the
  discriminant write in a separate statement and no live `p`.
- The same shape applies to `.glyph => |*p|`.

### 4.7 `253e4f9c3` / `1a88f3622` / `300b42f42a` stream fast paths

`consumeCsiParams` opens with `const p = &self.parser;`, accumulates locals, and on a
final byte writes back through `p` and then calls `self.csiDispatchFinal(c)` — which
takes `&mut self` and mutates `self.parser` again. Two live `&mut` to the same field.

- **Recommended shape:** finish all writes through `p`, `drop(p)` (end the borrow), then
  call `self.csi_dispatch_final(c)`. Rust NLL handles this if the borrow genuinely ends;
  do **not** paper over it with a raw `*mut Parser` held across the call.
- `csiEntryByte` is `inline fn` taking `*Self` and mutating `self.parser.*` fields; a
  `&mut self` method is fine there (no other live borrow).
- `consumeApcString` passes `input[0..end]` to `self.handler.vt(...)`. `input` is a
  caller-owned parameter, disjoint from `self` — safe.
- `print_slice` in `nextSlice` passes `cps[i..end]` where `cps` is a view of the local
  `cp_buf` — safe if `cp_buf` stays a local. If it is ever promoted to a `Stream`
  field, `&self.cp_buf[i..end]` + `&mut self.handler` become disjoint field borrows,
  which is still fine, but only via direct field access, not via a `&mut self` method.
- Reproduce the `comptime !@hasDecl(T, "vtRaw")` gate as a Rust trait-associated
  `const HAS_VT_RAW: bool` (or a marker trait). Silently dropping it strips raw events
  from the inspector (area F).

---

## 5. Cross-area dependencies and ordering

**Hard blockers (my work cannot land first):**

| Blocked item | Blocking area | What is needed |
|---|---|---|
| `446f80f4e` render restructure | **A** (`page.zig`) | `page.Mask(T, fields, len)` (upstream `page.zig:2303`) — absent in rustty |
| `446f80f4e` chunk loop | **A/B** (`PageList`) | `viewport_pin.pageIterator`, `node.serial`, `node.page()` |
| `89b103dd5` / `a3c1caba5` `Handler::resize` | **A** (`Terminal.zig` +193) | `Terminal.Resize { cols, rows, cell_size_px }`, `Terminal::resize(value)`, `width_px`/`height_px`, sync-output clearing |
| `c594031d5` cursor defaults | **A** (+144) & **E** (`stream_handler.zig` −68) | `Terminal::set_cursor_style`, terminal-owned cursor defaults, `fullReset` doing the reset |
| `6c8c07981` visibility | **A** (`Terminal.flags.visible`) & **E** (`termio/{Termio,Thread,message,stream_handler}`) | the flag plus the termio message that sets it |
| `39ae85f04` DECRQSS | **A** (`Terminal::print_attributes`) + `c594031d5` | `printAttributes(buf)` and `setCursorStyle` |
| `47e26df60` print batching | **A** (`cb2d78587` `printSliceFill`) | `Terminal::print_slice` |
| `7e14347c1` load factor | **A** (`page.zig` `GraphemeMap`, capacity math) | `Page` layout/size goldens move; `hyperlink.zig` Map → 80 |
| `fedd42e8d` backshift deletion | **A** (`page.rs` map callers) | pointer-stability audit (§4.5) |
| `e8525c0fd` kitty temp dir | **E** (`src/os`) + **C** (`c/*`, `include/`) | the explicit temp-dir parameter has to come from somewhere |
| `86f81fb5b` / `659a60ae5` | **A** | pure ripple of `max_scrollback_bytes` and `PageList.init(opts)` |

**Surfaces I feed into (area C / F should expect these):**

- `47d602c42`, `c3655ba25`, `6c8c07981`, `89b103dd5`, `39ae85f04` all have
  `c/terminal.zig` + `include/ghostty/vt/*.h` halves (~500 lines total) — area C.
- `1a88f3622`/`300b42f42a`'s `vtRaw` gate is load-bearing for the inspector — area F.
- `628adaf30`'s `LibEnum` progress-state move affects `c/terminal.zig` — area C.
- `osc/parsers/color.zig`'s `SegmentedList` now comes from the new
  `src/datastruct/segmented_list.zig` (533 lines) — `src/datastruct` owner.

**Recommended ordering within area D:**

1. **Now, standalone, cheap, and highest severity-per-line:**
   `5bc6588e4` (empty needle — fixes a live rustty-only bug), `627518447` (dangling-node
   window), `cb4c49fbf` (scalar decode divergence), `5caf20e3e`, `b5053153f`,
   the two `Selection.zig` mixed-width tests.
2. **Next, self-contained:** `9659167ec`+`bc8bb6c0f` (viewport fingerprint),
   `39ae85f04` (once `print_attributes` exists), `439d35e27`, `a65e11cc9`.
3. **After area A lands `Terminal.Resize` / cursor defaults:** `c594031d5`,
   `89b103dd5`, `a3c1caba5`, then `6c8c07981` (needs `flags.visible` + termio),
   then `47d602c42` / `c3655ba25` with their area-C halves.
4. **After area A lands `page.Mask`:** `446f80f4e`.
5. **The hash_map trio last** (`7e14347c1` → `65f953e8e` → `fedd42e8d`, in upstream
   order), because it moves `Page` layout *and* changes a pointer-stability contract
   that area A depends on. Gate on a Miri run of the full vt lane.
6. **BL-4 stream/SIMD family** (`253e4f9c3`, `1a88f3622`, `300b42f42a`, `f6f79acce`,
   `8c523ed03`, `47e26df60`, `083d9709b`) as one series with its bench corpus, per the
   existing BL-4 entry. Do not cherry-pick: the fast paths interlock and each one
   changes which action variants the handler must implement.

---

## 6. Open questions, with recommendations

1. **uucode revision bump.** Upstream moved `uucode.x.types` → `uucode.types` /
   `uucode.x.grapheme` → `uucode.grapheme` and changed the pinned tarball
   (`4749c4e93`: `uucode-0.2.0-ZZjBPqZV…` → `origin/main`:
   `uucode-2826a37a…` / hash `ZZjBPlK5…`), still labelled 0.2.0.
   `crates/rustty-unicode/src/generated/` holds tables generated against
   **uucode 0.2.0 at the old revision**. If the new revision changed any property data,
   rustty's tables are silently stale and `checksum.rs` will not catch it (it validates
   internal consistency, not upstream data).
   **Recommendation:** diff the two uucode tarballs' data outputs (or regenerate the
   three `*_data.rs` files against the new revision and byte-compare) before declaring
   `src/unicode` synced. Track this as an explicit Phase 3 item, not a footnote.
   *Blocked on:* fetching the new tarball; the dependency pin itself is area G.
2. **`page.Mask` ownership.** It lives in area-A `page.zig` but its only consumers
   today are area-D `render.zig` (and, upstream, `Terminal.zig`'s print fast paths).
   **Recommendation:** area A ports `Mask` with a raw-base API (`*const T` + len, not
   `&[T]`) so area D can use it while a `*mut Page` alias is live (§4.1). Agree the
   signature before either side starts.
3. **`Limits.temporary_file` and the temp directory.** Upstream's restructure removes a
   process-env lookup from a path-validation check, which reads as a deliberate
   hardening, but it lands inside the Zig-0.16 commit, so intent is inferred rather
   than stated. **Recommendation:** treat it as a security-relevant change, port the
   explicit-directory form, and coordinate with area E on where the directory comes
   from — do not keep the env lookup "temporarily".
4. **`semantic_failure` and the try/log split.** `439d35e27` re-partitions which
   handler errors are "unhandled". Porting the flag without the
   `setAttribute`/`windowTitle`/`reportPwd` promotions (and *without* promoting
   `colorOperation`/`kittyColorOperation`/`clipboardContents`, which stay logged)
   would give a flag that never fires — the exact "vacuous guard" failure mode
   `.port/phase-3/README.md` warns about. **Recommendation:** one commit, all hunks,
   with the upstream sticky-flag test.
5. **Where does `Handler::resize` live in rustty?** Upstream puts it on the
   `stream_terminal.Handler` because it needs `effects.write_pty`. rustty's
   `renderer`/`core` split has previously pulled such seams elsewhere (cf. the renderer
   `Thread` living in `rustty-core`). **Recommendation:** keep it on
   `stream_terminal::Handler` — it touches only `Terminal` + `Effects`, both already in
   `rustty-vt`, and moving it would force a `rustty-vt` → apprt edge.
6. **Missing guarding tests are the norm, not the exception.** Of the 26 ABSENT/PARTIAL
   rows above, **every single one** has "none — needs one" in the test column. That is
   expected for absent features, but two rows deserve tests even though upstream ships
   none: the kitty eviction **ordering** (§3.6) and the hash-map **canonical placement
   invariant** as a Rust `debug_assert`-style helper rather than a test-only function
   (§2.1). **Recommendation:** make both explicit deliverables of their rows.
