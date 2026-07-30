# Phase 3 area B — compression: re-vetting assessment

**Range:** `4749c4e93..6ad1fe7d8` (oracle `origin/main`, 2026-07-29)
**Assessed against:** rustty `main` @ `b258787` (this worktree's base)
**Author lane:** Phase 3 area B (compression). Assessment only — no production code changed.

---

## 1. Summary — verdict

**BL-1's codec and representation core is genuinely complete, and I can prove it
byte-for-byte. BL-1's *benchmark* surface is NOT complete: two of the four new
`src/benchmark` files are wholly absent and two pre-existing benchmarks are
stuck at their pinned-baseline shape. And the *coverage* around the codec has a
real, demonstrated hole: nothing in the repo would notice if rustty's encoder
started emitting a different byte stream from upstream's.**

Numbers:

| Thing | Upstream | rustty | Verdict |
|---|---|---|---|
| `compress/lz4.zig` | 891 lines, 12 tests + 1 import-test | `compress/lz4.rs` 999 lines, 13 tests | **PORTED**, byte-exact (proven, §2.1) |
| `compress/lz4_differential.zig` | 493 lines, 2 tests | `compress/lz4_differential.rs` 609 lines, 2 tests | **PORTED**, corpora diverge (§4.1) |
| `compress/Page.zig` | 398 lines, 4 tests | `compress/page.rs` 476 lines, 4 tests | **PORTED**, one numeric divergence (§4.2) |
| `compress.zig` | 15 lines | `compress.rs` 18 lines | **PORTED** |
| `compress/AGENTS.md` | 90 lines | — | **ABSENT** (§4.6) |
| `mem.zig` | 222 lines, 3 tests | `src/mem.rs` 338 lines, 3 tests | **PORTED**, `cfg(test)` divergence (§4.3) |
| PageList compression policy | ~470 lines in `PageList.zig` | `pagelist/compress.rs` 688 lines | **PORTED** |
| `benchmark/PageCompression.zig` | 521 lines, 5 modes, 5 opts, 2 tests | `page_compression.rs`, 5 modes, 5 opts, 2 tests | **PORTED** |
| `benchmark/ScrollbackCompression.zig` | 344 lines, 5 modes, 4 opts, 3 tests | `scrollback_compression.rs`, 5 modes, 4 opts, 3 tests | **PORTED** |
| `benchmark/HyperlinkMap.zig` | 155 lines, 2 modes, 4 opts, 1 test | — | **ABSENT** (§3.1) |
| `benchmark/ApcParser.zig` | 144 lines, 1 test | — | **ABSENT** (§3.2) |
| `benchmark/ScreenClone.zig` (+111/−2) | 6 modes + `loops` | 3 modes, no `loops` | **PARTIAL** (§3.3) |
| `benchmark/TerminalStream.zig` (+27/−31) | full `TerminalStream` handler, 64 KiB unbuffered | print-only `Handler`, 4 KiB buffered | **PARTIAL** (§3.4) |

Live checks run: `cargo check -p rustty-vt` clean; `cargo test -p rustty-vt
--lib --release compress::` → **19/19 pass**; `cargo test -p rustty-bench --lib
--release` → **14/14 pass**.

**The amendment question the brief flagged is closed, positively.** The three
upstream commits that amended compression *after* its introduction —
`b9bb50c83` (+395/−53 to `lz4.zig`), `9a4bd2120` (+427/−227 to `lz4.zig`, plus
`lz4_differential.zig` and `AGENTS.md`), and `e8525c0fd` (Zig 0.16, 1 line in
`lz4_differential.zig`) — are all reflected. I did not take this on faith from
commit lists: I compared rustty against `git show origin/main:<path>`, i.e. the
**final** state, which subsumes every amendment by construction, and then proved
functional equality independently (§2.1). `e8525c0fd`'s hunk is
`std.process.hasEnvVarConstant` → `std.testing.environ.containsConstant`, a pure
Zig-0.16 stdlib rename: **N/A**.

Three `MAIN_PARITY.md` rows are **stale in the "claims work open that shipped"
direction** and should be corrected in Phase 3 (§5.2).

---

## 2. What I proved, and how

### 2.1 The LZ4 codec is byte-exact with upstream — 7,976/7,976 points, zero divergences

Reading two implementations side by side establishes plausibility, not identity.
So I built a real differential: upstream's `lz4.zig` compiled standalone
(`zig build-exe`, which upstream's own `compress/AGENTS.md` documents as
supported because the codec depends only on `std`) against **rustty's actual
`compress/lz4.rs`**, compiled standalone with `rustc` (it has no dependencies
beyond `core`/`std`). Both harnesses implement the identical deterministic
corpus generator, so both walk the same bytes.

Harnesses: `/tmp/b-comp/z/harness.zig`, `/tmp/b-comp/rs/main.rs` (transient; not
committed — see §5.4 for the recommendation to make this permanent).

| Phase | What | Points | Result |
|---|---|---|---|
| 1 | 8 corpus families (zeros, cell-structured, random, runs, periodic, sparse, text, ramp) × 35 boundary sizes 0…409,600 | 280 | identical |
| 2 | randomized `(family, size, seed)` triples, sizes 0…409,600 | 4,000 | identical |
| 3 | every match period 1…96 over 200 KiB — exercises every `copy_match` strategy and both wide-copy strides | 96 | identical |
| 4 | **decoder**: 60 valid blocks × 60 deterministic corruptions (bit flips, byte overwrite, 16-byte splices, random-prefix truncation) × rotating exact/short/long output buffers | 3,600 | identical |
| | **total** | **7,976** | **0 divergences** |

Comparison key per point: encoded length **and** an FNV-1a hash of the encoded
bytes (phases 1–3); for phase 4, the exact `@errorName`/`DecompressError`
variant on failure, or decoded length + FNV-1a hash of the decoded bytes on
success.

This is stronger than it may look:

- **Phases 1–3 pin the encoder's byte stream**, not merely round-trip identity.
  A codec that round-trips its own output but picks different matches would show
  up here as a length/hash mismatch. It does not.
- **Phase 4 pins the decoder's complete observable behaviour on malformed
  input** — the exact error taxonomy (`TruncatedInput` / `InvalidOffset` /
  `OutputTooSmall` / `OutputSizeMismatch`, the four-variant set `9a4bd2120`
  settled on), *and* the byte-exact output of successful decodes of corrupted
  blocks. That last part matters specifically because `9a4bd2120`'s optimization
  is *blind wide copies that deliberately write scratch bytes past a run's
  logical end*. Two implementations can agree on valid blocks and disagree on
  exactly which scratch bytes survive. They do not disagree.
- Both sides ran with runtime safety on (Zig `-O ReleaseSafe`; Rust
  `-C debug-assertions=on` plus Rust's always-on slice bounds checks). **Zero
  panics on either side across 3,600 corrupted decodes** — independent
  corroboration of the memory-safety property `compress/AGENTS.md` demands.

Additionally, every Rust encode in phases 1–3 was round-tripped and compared to
its input in the same run (4,376 round-trips, all clean).

### 2.2 Line-by-line structural review (independent of §2.1)

I read both sides of `lz4.zig`/`lz4.rs` in full and confirmed every constant and
every guard path: `MAX_INPUT_SIZE 0x7E000000`, `MIN_MATCH 4`, `LAST_LITERALS 5`,
`MATCH_FIND_LIMIT 12`, `HASH_LOG 12`, `HASH_MULTIPLIER 2_654_435_761`;
`compress_bound` = `len + len/255 + 16`; the two-position packed hash entry with
the `0xFFFF` sentinel and the "older candidate must also match one byte beyond
`MIN_MATCH`" collision rule; `match_begin`'s `leading_zeros/8` and
`match_end_position`'s `trailing_zeros/8`; `advance_search`'s 1024-byte
threshold and `1 + attempts/64` step; the `match_end - 2` seed; `copy_match`'s
`offset >= match_len && match_len >= 64` memcpy arm, the `1|2|4|8` pattern-word
arm with its three multiplication constants, and the `>=16`/`>=8` stride arms
with `saturating_sub` limits; `decompress`'s `>= 16` literal margin and
`match_nibble != 15 && offset >= 8 && output.len()-op >= 18` match margin.

Semantics I checked explicitly rather than assumed:

- Oracle `@as(u32, @truncate(table[hash])) << 16` vs Rust `table[hash] << 16` —
  the `@truncate` is a u32→u32 no-op; both languages discard bits shifted out of
  a `<<` (neither traps on lost value bits). Equivalent.
- The three `copy_match` pattern multiplications cannot overflow `u64` at their
  maxima (`0xFF × 0x0101…01`, `0xFFFF × 0x0001…01`, `0xFFFF_FFFF ×
  0x0000_0001_0000_0001` all equal exactly `u64::MAX`), so Rust's plain `*`
  under `overflow-checks = true` matches Zig's trapping `*`. The rustty comment
  states this; I verified the arithmetic.
- `match_end_position` replaces the oracle's trapping `limit - position` with
  `limit.saturating_sub(position)` **plus** a `debug_assert!(position <= limit)`,
  so the saturating form cannot silently absorb an invariant break.
- `emit_sequence`'s `@intCast(ip - match_pos)` trap is preserved as
  `debug_assert!(ip - match_pos <= u16::MAX as usize)`.

Zig `assert` → Rust `debug_assert!` throughout is the standard port mapping and
is a genuine (small) strength reduction in release builds; it is uniform across
the port and I am not raising it per-site.

### 2.3 `lz4_differential_light` is a REAL guard — measured, with the boundary of what it catches

The brief warns that WS7 guard tests were found vacuous. This one is not, and I
did not just assert that: I built the differential suite standalone against a
mutable copy of `lz4.rs` and injected seven faults.

| Injected fault | `lz4_differential_light` |
|---|---|
| `LAST_LITERALS` 5 → 4 | **FAILS** ✅ |
| `MATCH_FIND_LIMIT` 12 → 11 | **FAILS** ✅ |
| decoder literal fast-path margin `>= 16` → `>= 15` | **FAILS** ✅ |
| decoder match fast-path `offset >= 8` → `offset >= 4` | **FAILS** ✅ |
| `copy_match` memcpy threshold 64 → 8 | passes ❌ |
| `HASH_LOG` 12 → 11 | passes ❌ |
| `advance_search` 1024 → 512 | passes ❌ |

So: **the light differential is a real, load-bearing guard for format
conformance and for every decoder bounds/margin path.** Its independent format
walker (`expect_valid_block`) is what catches the first two; the wrong-size and
corruption passes catch the second two. It is faithfully ported, including the
walker's independently-declared `LAST_LITERALS`/`MATCH_FIND_LIMIT` constants
(which is precisely why fault 1 and 2 are caught rather than cancelling out).

**The three it misses are the finding.** They are ratio/output-shape faults that
preserve round-trip identity *and* format validity — exactly the class
upstream's `compress/AGENTS.md` says must be guarded by other means: *"When a
change should not alter compressor output, prove it: compare encoded sizes (or a
sequence-count fingerprint) on the same corpus before and after. Ratio drift is
a functional change, not noise."* rustty has **no such guard**. I measured the
consequence: with `HASH_LOG` 12 → 11, **1,929 of 7,976** differential points
diverge from upstream's byte stream, and the full rustty test suite stays green.
See §4.5.

(Honesty note on my own corpus: `advance_search` 1024 → 512 produced **0**
divergences on my 7,976 points, so my corpus does not reach the long
incompressible-literal-run shape that knob controls. My differential is broad,
not exhaustive.)

---

## 3. Per-change verdict table

Cost: S ≤ ½ day, M ≤ 2 days, L > 2 days.

### Files upstream ADDED under `src/terminal/compress` + `src/terminal/mem.zig`

| Upstream commit(s) | What changed | rustty status | Evidence | Guarding test / gate | Cost | Risk |
|---|---|---|---|---|---|---|
| `c62c15984` → amended by `b9bb50c83`, `9a4bd2120`, `e8525c0fd` | `compress/lz4.zig` (+891): raw LZ4 block codec, allocation-free; fast compressor w/ 2-position 12-bit hash table; blind-wide-copy decoder | **PORTED** — final state, all amendments included | `crates/rustty-vt/src/compress/lz4.rs:56` `MAX_INPUT_SIZE` … `:520` `copy_match` … `:378` `decompress`. Byte-exact vs `origin/main:src/terminal/compress/lz4.zig` on 7,976 differential points (§2.1) | `compress::lz4::tests::*` (12 ported + 1 added), 13/13 pass. `lz4_differential_light` is a proven-real guard (§2.3) | — | — |
| `ebc3ffd22` | `compress/Page.zig` (+398): compressed page retaining its resident mapping; `requiredScratch`/`init`/`deinit`/`restore`/`cloneBuf` | **PORTED** (1 numeric divergence, §4.2) | `crates/rustty-vt/src/compress/page.rs:52` `CompressedPage`, `:76` `required_scratch`, `:113` `init`, `:174` `restore`, `:199` `clone_buf` | `compress::page::tests::*` — all 4 oracle tests ported, 4/4 pass | — | — |
| `f6fd4cb08` | `terminal/mem.zig` (+222): `DecommitMode`, `canReclaim`, `decommit`, `recommit` (Linux `MADV_DONTNEED`; Darwin `FREE_REUSABLE`/`FREE_REUSE`) | **PORTED** (`cfg(test)` divergence, §4.3). NOTE: rustty path is `crates/rustty-vt/src/mem.rs`, **not** `pagelist/mem.rs` as the area brief states | `crates/rustty-vt/src/mem.rs:33` `DecommitMode`, `:97` `can_reclaim`, `:154` `decommit`, `:249` `recommit`; madvise constants pinned at `:50-57` | `mem::tests::*` — all 3 oracle tests ported | — | — |
| `9a4bd2120` | `compress/lz4_differential.zig` (+493): 7 generators, boundary sizes, structural walker, mutation safety, light + exhaustive budgets | **PORTED** (corpora diverge, §4.1) | `crates/rustty-vt/src/compress/lz4_differential.rs:51` `BOUNDARY_SIZES` (38 entries, exact), `:496` `LIGHT_BUDGET`, `:509` `EXHAUSTIVE_BUDGET` (all knobs and both period lists exact), `:355` `expect_valid_block` | Real test names are **`lz4_differential_light`** (`:590`) and **`lz4_differential_exhaustive`** (`:595`). Light: proven real (§2.3). Exhaustive: **vacuous in every automated run** (§4.4) | — | — |
| `9a4bd2120` | `compress/AGENTS.md` (+90): codec priorities, testing/benchmark/verification doctrine | **ABSENT** | `find . -name AGENTS.md` → 7 files, none under `crates/rustty-vt/src/compress/` (nearest sibling precedent: `crates/rustty-vt/src/apc/glyph/AGENTS.md`) | none — needs one; its ratio-drift rule is the missing gate in §4.5 | S | Low (doc), but it is the doc that *specifies* the missing gate |
| `c62c15984` | `compress.zig` (+15): namespace module | **PORTED** | `crates/rustty-vt/src/compress.rs:10,17` — `pub mod lz4; pub mod page;` + `#[cfg(test)] mod lz4_differential;` at `:14`, mirroring `lz4.zig`'s trailing `test { _ = @import("lz4_differential.zig"); }` | n/a (module decl) | — | — |
| `e8525c0fd` | `lz4_differential.zig`: `std.process.hasEnvVarConstant` → `std.testing.environ.containsConstant` | **N/A** — Zig 0.16 stdlib rename | rustty uses `std::env::var_os("RUSTTY_LZ4_SLOW")` at `lz4_differential.rs:601` | n/a | — | — |

### PageList compression policy (compression surface of `PageList.zig`; bodies are area A)

| Upstream commit(s) | What changed | rustty status | Evidence | Guarding test / gate | Cost | Risk |
|---|---|---|---|---|---|---|
| `421fe8dab`, `9156ada16`, `70e788f06`, `f8c217e55`, `461562ca4`, `181254d36`, `95685afd2`, `e7969ed43`, `b9bb50c83` | `CompressionScratch`, `IncrementalCompressionState`/`Flags`, `CompressionIterator`, `compress(.incremental/.drain/.full)`, `compressIncremental`, `compressFull`, `compressPage`, `MemoryStats`/`memoryStats`, `compressPage_tw` failure injection | **PORTED** — line-for-line faithful | `crates/rustty-vt/src/pagelist/compress.rs:26` `CompressionScratch`, `:116` `IncrementalCompressionState`, `:235/244/271` iterator `init`/`next`/`done`, `:366` `compress_page`, `:485` `compress`, `:524` `compress_incremental`, `:623` `memory_stats`, `:676` `compress_full`; `:301` + `:319` tripwire. `INCREMENTAL_COMPRESSION_MAX_INSPECTED` = 8 ✓; `estimated_savings` uses `saturating_sub` for `-|` ✓ | `pagelist::tests::*` compression tests (incl. `pagelist_compress_page_round_trips_a_cold_history_page`, `pagelist_compress_drain_finishes_in_one_call`) | — | — |
| `d0a26d146` | `compact` + `increaseCapacity` must `markActivity()` after publishing a replacement page, without resetting the continuation marker | **PORTED** — **`MAIN_PARITY.md:76` is STALE** (says "missing BL-1") | Call sites: `pagelist/grow.rs:576` (`increase_capacity`), `:666` (`compact`). Oracle's test ported verbatim at `pagelist/tests.rs:2365` `pagelist_replacements_preserve_compression_continuation_and_mark_activity` | Real guard: asserts `activity_serial` 42→43 while `last_serial`/`next_serial` are preserved. Would fail if either `mark_activity()` were dropped, or if it were replaced by a `reset()` | S (ledger fix only) | — |
| `0d015b2fc` | search must not restore compressed pages (`search/{active,pagelist,sliding_window,viewport}.zig`, +153/−26) | **PORTED** — **`MAIN_PARITY.md:134` is STALE** (says "missing BL-1"). Cross-area: bodies are area A | `crates/rustty-vt/src/search/sliding_window.rs:152,626,688,891,910,936`; `search/active.rs:125`; `search/viewport.rs:252,295` — all carry `oracle 0d015b2fc` provenance | Area A owns verifying the search test set | S (ledger fix) | — |
| `172f15da3` | expose compression through libghostty-vt | **PORTED** (area C) | `MAIN_PARITY.md:126` documents it; `crates/rustty-vt-c/src/terminal.rs:2717,2764` | area C | — | — |
| `6d5dda40d` | inspector page-compression memory rows | **PORTED** (area F) — **`MAIN_PARITY.md:127` is STALE** (says "missing BL-1") | `crates/rustty-inspector/src/widgets/pagelist.rs:62,413,466,2464` | area F | S (ledger fix) | — |
| `0fb89f4ff` | `scrollback-compression` config option + renderer thread gating | **PORTED** (areas E/G) | `MAIN_PARITY.md:128`; `rustty-config/src/config_struct.rs:10648`, `renderer_config.rs:228`, `rustty-core/src/renderer_seam.rs:1759,2300,2333` | areas E/G | — | — |
| `25e624569` | renderer must not starve scrollback compression | **PORTED** (area E) | `MAIN_PARITY.md:125`; `rustty-core/src/renderer_seam.rs:1760` | area E | — | — |

### `src/benchmark` (whole directory is area B)

| Upstream commit(s) | What changed | rustty status | Evidence | Guarding test / gate | Cost | Risk |
|---|---|---|---|---|---|---|
| `95685afd2` (+`86f81fb5b`) | `PageCompression.zig` (+521): 5 modes, 5 options, ring-buffered `store`, `report` | **PORTED** — complete | `crates/rustty-bench/src/page_compression.rs:227` `Mode` (all 5), `:148` `Options` (all 5: `mode`, `loops`, `page-size` 400 KiB, `retained-pages` 25, `data`), `:98` `MAX_DATA_SIZE` 64 MiB, `:787` `PageIterator`. Both oracle tests ported (`:849`, `:866`), pass | `page_compression`, `page_compression_store_retains_exact_encoded_allocations` — the second is real: asserts `stored_next == 1` after 3 pages into a 2-slot ring and decodes each retained block | — | — |
| `95685afd2` (+`86f81fb5b`) | `ScrollbackCompression.zig` (+344): 5 modes, 4 options, `visitColdPages` via `Node.page` | **PORTED** — complete | `scrollback_compression.rs:189` `Mode` (all 5), `:114` `Options` (all 4), `:416` `visit_cold_pages`, `:448` `step_report`. All 3 oracle tests ported (`:482`, `:498`, `:553`), pass | `scrollback_compression_restores_cold_terminal_pages` and `..._drains_incremental_compression_steps` are real: the first asserts `compressed_pages > 0` then that a second `Full` pass yields the same count (i.e. traversal actually restored through `Node::page`); the second asserts drain and full converge to an identical `MemoryStats`. Both would fail if `visit_cold_pages` inspected metadata instead of restoring | — | — |
| `86f81fb5b` | `ScrollbackCompression.create`: `max_scrollback` → `max_scrollback_bytes` | **PARTIAL / blocked** | `scrollback_compression.rs:234-238` documents it: rustty's `terminal::Options` is still at the pinned shape (single `max_scrollback` in bytes) | none needed at bench level; the real work is the area-A `Terminal.zig` bytes/lines split (`86f81fb5b`, `5b2d3b7df`, `10bc43420`, `f4c68d65e`, …) | S (after area A) | Low. **Cross-area dependency on A** |
| `7e14347c1` | **`HyperlinkMap.zig` (+155, NEW)** + `cli.zig` + `main.zig` registration | **ABSENT** | `grep -rn "HyperlinkMap\|hyperlink-map" crates/rustty-bench/src/` → **no matches**. `crates/rustty-bench/src/cli.rs:22-41` `Action` has 9 variants; upstream has 11 | none — needs one (the existing `cli::tests::bench_registers_compression_actions` only asserts the two *compression* actions) | M | Low functional / Medium parity. Depends only on `Page` + `hyperlink`, both ported → portable today |
| `f6f79acce` | **`ApcParser.zig` (+144, NEW)** + `cli.zig` registration | **ABSENT**, and **blocked** | `grep -rn "ApcParser\|apc-parser" crates/rustty-bench/src/` → **no matches**. Blocker: the benchmark's `Handler` dispatches `.apc_put_slice`; `grep -rn "apc_put_slice\|feed_slice" crates/rustty-vt/src/` → **no matches** | none — needs one | M (after A) | Low functional. **Cross-area dependency on A**: `f6f79acce`'s vt-stream bulk-slice APC dispatch must land first |
| `446f80f4e` | `ScreenClone.zig` (+111/−2): `loops: u32` option; 3 new modes `render-locked`, `render-clean`, `render-partial`; `render` iteration count 1,000 → 50,000 × loops | **PARTIAL** | `crates/rustty-bench/src/screen_clone.rs:80-99` — `Mode` has only `Noop`/`Clone`/`Render`; `from_name` maps only 3 tokens; `Options` (`:40`) has no `loops`. Missing hunks enumerated in §3.3 | none — needs one | M | Low functional / Medium parity. **Cross-area dependency on A/E**: needs `RenderState::begin_update` and `Pin::mark_dirty` |
| `4c5b1d5d5`, `258de36d1`, `f2a7652ab` | `TerminalStream.zig`: replace the print-only local `Handler` with the full `terminal.TerminalStream`; unbuffered reader + 64 KiB chunks matching termio `buffer_capacity` | **PARTIAL** | `crates/rustty-bench/src/terminal_stream.rs:262` still defines the local print-only `Handler`; `:31` `use rustty_vt::stream::Stream`; `:223` `let mut buf = [0u8; 4096]`. Missing hunks in §3.4 | none — needs one | M | Low functional / Medium parity (the benchmark measures materially less work than upstream's). **Cross-area dependency on A**: needs `terminal::TerminalStream` (the readonly full handler) |
| `e8525c0fd`, `048619a6b` | `Benchmark.zig` `std.time.Instant` → `std.Io.Timestamp`; `CApi.zig`/`cli.zig` `global.alloc()`/`std.process.Init.Minimal`; `CodepointWidth`/`GraphemeBreak`/`IsSymbol`/`OscParser`/`TerminalParser`/`options.zig` `std.fs.File` → `std.Io.File` + `global.io()` | **N/A** — Zig 0.16 `std.Io` migration, no Rust counterpart | Read the complete diff of all 10 files; every hunk is an `Io`/`global` threading change or an `ArgIterator` rename | n/a | — | — |

---

## 4. Partials and divergences — exact missing hunks

### 3.1 `HyperlinkMap.zig` — wholly ABSENT (upstream `7e14347c1`)

Nothing exists. Full surface to port (`origin/main:src/benchmark/HyperlinkMap.zig`, 155 lines):

- fields `opts`, `page: terminal.Page`, `link_id: hyperlink.Id`, `entry_count: usize`
- `Options`: `entries: u16 = 4096` (power of two, ≥ 16), `@"load-percent": u8 = 100` (values > 100 clamped), `loops: u16 = 1`, `mode: Mode = .churn`
- `Mode`: `lookup` (look up every populated hyperlink cell), `churn` (remove + reinsert every populated cell)
- `create`/`destroy`/`benchmark`/`setup`/`teardown` + the two step fns
- 1 test
- registration: `cli.zig` `Action.@"hyperlink-map"` + `Action.Struct` arm; `main.zig` `pub const HyperlinkMap`

rustty registration sites: `crates/rustty-bench/src/cli.rs:22` (`Action`), `:47`
(`from_name`), `:209` (`main_action` dispatch), `crates/rustty-bench/src/lib.rs`.
Dependencies (`Page`, `hyperlink`) are already ported, so this is unblocked.

### 3.2 `ApcParser.zig` — wholly ABSENT and BLOCKED (upstream `f6f79acce`)

Nothing exists. Full surface (144 lines): `Options { data: ?[]const u8 }`, a
`Stream(Handler)` whose `Handler` holds `apc: terminalpkg.apc.Handler` and
dispatches `.apc_start` / `.apc_put` / **`.apc_put_slice`** / `.apc_end`
(parsing then immediately `deinit`-ing each completed command), 1 test, plus the
`cli.zig` registration.

**Blocker, verified:** `apc_put_slice` does not exist in rustty —
`grep -rn "apc_put_slice\|ApcPutSlice\|feed_slice" crates/rustty-vt/src/`
returns nothing. `f6f79acce` ("terminal: dispatch APC string bytes in bulk
slices") is an unported vt-stream change owned by area A. Port order: A's
`f6f79acce` → then this benchmark.

### 3.3 `ScreenClone.zig` — PARTIAL (upstream `446f80f4e`)

Present: `Noop`, `Clone`, `Render`. Missing, hunk by hunk:

1. `Options.loops: u32 = 1` — "Multiplier on the number of iterations each step
   runs. This is useful to make a benchmark run long enough for profiling."
   Missing at `screen_clone.rs:40`.
2. `Mode.@"render-locked"` — "Like render, but only the portion of the render
   state update that requires holding a terminal lock (`beginUpdate`). The
   deferred work (`endUpdate`) is excluded since it happens outside of any
   locks." → `stepRenderLocked`: initial `state.update`, then
   `50_000 * loops` iterations of `state.screen = .alternate;
   state.beginUpdate(alloc, &self.terminal)`.
3. `Mode.@"render-clean"` — "RenderState update with no changes to the
   terminal… cursor blink, mouse movement." → `stepRenderClean`: initial
   `update`, then `3_000_000 * loops` iterations of plain `state.update`.
4. `Mode.@"render-partial"` — "RenderState update where a single row is dirty…
   shell prompt or TUI updating a small portion of the screen." →
   `stepRenderPartial`: initial `update`; `pages.pin(.{ .active = .{ .x = 0,
   .y = self.terminal.rows / 2 } }).?`; then `2_000_000 * loops` iterations of
   `pin.markDirty(); state.update(...)`.
5. `stepRender`'s loop bound `for (0..1000)` → `for (0..50_000 * @as(u64,
   self.opts.loops))`, and `doNotOptimizeAway(state)` → `(&state)`.
6. Registration of the three modes in `benchmark()`'s `stepFn` switch and in
   `Mode.from_name`.

**Cross-area dependency:** items 2 and 4 need `RenderState::begin_update` and
`Pin::mark_dirty` from `446f80f4e`'s `src/renderer/render.zig` half (+913/−168,
area A/E). Item 1, 3, 5 are unblocked.

### 3.4 `TerminalStream.zig` — PARTIAL (upstream `258de36d1` + `4c5b1d5d5` + `f2a7652ab`)

Missing hunks:

1. **The handler swap.** Upstream deleted the local print-only `Handler` struct
   entirely and set `const Stream = terminalpkg.TerminalStream`, constructed
   with `.initAlloc(alloc, .init(&ptr.terminal))` and torn down with
   `self.stream.deinit()` in `destroy`. rustty still has the print-only
   `Handler` at `terminal_stream.rs:262` (its own comment: "only acts on
   `.print`"). Consequence: rustty's `terminal-stream` benchmark measures
   materially less work than upstream's — no styles, cursor movement, erases, or
   modes.
2. **The read path.** `f.reader(global.io(), &read_buf)` → `f.reader(global.io(),
   &.{})` (unbuffered, so `readSliceShort` reads straight into `buf`, "avoiding
   a per-chunk memcpy through an intermediate reader buffer that would pollute
   the measurement"), and `var buf: [4096]u8` → `[64 * 1024]u8` "matches the
   read buffer size used by the real IO thread (see termio `Exec.zig`
   `buffer_capacity`)". rustty is at `4096` (`terminal_stream.rs:223`).
3. The module doc-comment rewrite (the "isn't the full terminal handler…"
   caveat is replaced by the "full readonly terminal stream handler" text and a
   pointer to `terminal-parser`/`osc-parser` for isolated measurements).

**Cross-area dependency on A:** `terminal.TerminalStream` (upstream
`stream_terminal.zig`, +1,103/−130 in this range) must be ported first.

### 4.1 `lz4_differential.rs` — corpora are NOT stream-faithful (divergence)

`lz4_differential.rs:61-63` claims the PRNG "is Zig's `std.Random.DefaultPrng`
(xoshiro256++) with its SplitMix64 seeding, **plus the handful of `std.Random`
helpers the oracle's generators call**." The core generator is faithful
(`:83-96` is xoshiro256++; `:70-81` is SplitMix64). **The helpers are not**, and
that makes the generated corpora different byte sequences from upstream's:

- `Prng::bytes` (`:102`) consumes **one full `u64` per byte**; Zig's
  `Random.bytes` fills 8 bytes per `u64` draw. → 8× different stream consumption.
- `uint_less_than` (`:109`) is `next() % less_than` (modulo-biased); Zig's is an
  unbiased multiply/reject algorithm consuming a different number of draws.
- `Generator::Cells` (`:203`) uses `random.next() % 95` / `% 4` where the oracle
  uses `random.uintLessThan(u8, 95)` / `(u8, 4)` — different draw widths.
- `int_u8` (`:98`) is `next() as u8`; Zig's `Random.int(u8)` takes a byte from
  its internal byte stream.

Same issue, smaller, in `lz4.rs:973` `lz4_round_trips_deterministic_random_inputs`,
which honestly documents replacing xoshiro with a xorshift64\*.

**Assessment: acceptable but must be recorded.** Every assertion in the suite is
structural (round-trip identity, format validity, wrong-size rejection,
memory-safety under corruption), so the property is preserved and the suite is
demonstrably load-bearing (§2.3). But rustty does **not** exercise the specific
input bytes upstream exercises, so an input-shape-specific bug upstream's seed
happens to hit would not be reproduced. The remedy is either stream-faithful
helpers (S, and the doc-comment's current claim would then be true) or a
corrected doc-comment (XS). **The doc-comment as written overstates fidelity and
should be fixed either way.**

### 4.2 `required_scratch` break-even threshold differs by 8 bytes (divergence, measured)

Upstream: `representation_overhead = @sizeOf(compress.Page) - @sizeOf(TerminalPage)`
= `@sizeOf([]u8)` + `@sizeOf(std.mem.Allocator)` = 16 + 16 = **32** on 64-bit.
rustty: `size_of::<CompressedPage>() - size_of::<Page>()` = `size_of::<Vec<u8>>()`
= **24** (the `alloc: Allocator` field is unnecessary because `Vec` carries its
allocator — documented at `page.rs:38-41`).

Measured on rustty: `required_scratch(1_000_000) = 999_975` ⇒ overhead **24**;
`size_of::<CompressedPage>()` = 424.

Effect: rustty's break-even is 8 bytes more permissive, so a page whose best
encoding lands in the 8-byte window `[raw-32, raw-24)` is compressed by rustty
and declined by upstream. In practice encodings are 10–30 % of raw, so this
window is essentially never hit — but it is a **numeric divergence in a derived
constant** and the D3 constants-parity gate's subject matter. The overhead
*computation* is correct-by-construction (in-language representation size is the
faithful intent); what is missing is the note recording the resulting threshold
shift. Cost XS (comment), risk negligible.

Also note `compressed_page_requires_the_maximum_useful_scratch` asserts
`required == memory_len - (size_of::<CompressedPage>() - size_of::<Page>()) - 1`,
which restates the implementation and is therefore **tautological**: it cannot
detect an overhead change. Its non-vacuous half is the second assertion, that
`init` with `required - 1` returns `OutputTooSmall` and leaves the source bytes
untouched — that part is real.

### 4.3 `mem.rs`: `cfg(test)` is not transitive across crates (divergence)

`mem.rs:108` (`can_reclaim`) and `:173` (`decommit`) branch on
`cfg!(test) || cfg!(miri)`, porting `builtin.is_test`. In Zig, `zig build test`
compiles the *whole* terminal module with `is_test = true`, so the benchmark's
own tests see the zeroing simulation. In Rust, `cfg(test)` is true only when
compiling **rustty-vt's own** test harness. When `rustty-bench`'s tests link
rustty-vt as a dependency, `cfg(test)` is **false**, so:

`scrollback_compression_restores_cold_terminal_pages` and
`..._drains_incremental_compression_steps` exercise the **real kernel
`MADV_FREE_REUSABLE` path** on real `rustty-os` mappings, whereas their oracle
counterparts exercise the memset simulation.

They pass here (macOS/arm64, 14/14 green). But `..._restores_cold_terminal_pages`
asserts `compressed_pages > 0`, which now depends on a real kernel call
succeeding: on any target where `can_reclaim(Strict)` is false (non-Darwin,
non-Linux, or non-64-bit — e.g. a wasm32 or 32-bit runner), `compress` returns
`Unsupported`, `compressed_pages == 0`, and **the test FAILS** where upstream's
would pass. That is a latent portability failure, not a correctness bug.
Cost S (gate the simulation on something crate-external, e.g. a `cfg(feature =
"test-decommit-sim")` that rustty-bench's dev-deps enable, or `cfg(any(test,
debug_assertions))` if that is acceptable); risk Low today, Medium if CI ever
adds a non-Darwin/Linux target.

Also minor: Zig's `assert` here is `quirks.inlineAssert` (active in ReleaseSafe);
rustty uses `debug_assert!` (compiled out in release).

### 4.4 `lz4_differential_exhaustive` is vacuous in every automated run

`lz4_differential_exhaustive` (`lz4_differential.rs:595`) returns early unless
`RUSTTY_LZ4_SLOW` is set. Verified: `grep -rn "RUSTTY_LZ4_SLOW"` over all
`*.sh`, `*.yml`, `*.yaml`, `*.toml`, `*.py`, `*.md`, `*.rs` finds it **only** in
the file itself and in the WS7 plan doc — **no gate, no `ci.sh` entry, no GitHub
workflow ever sets it.** Observed directly: `cargo test -p rustty-vt --lib
--release compress::` reports `compress::lz4_differential::lz4_differential_exhaustive
... ok` with the whole 19-test suite finishing in 0.03 s.

Upstream has the same shape (`git grep -l GHOSTTY_LZ4_SLOW origin/main` → only
`AGENTS.md` and `lz4_differential.zig`), **so this is faithful, not a port
defect.** But it is worse in one specific way: Zig's `error.SkipZigTest` makes
the test report **SKIPPED**, while Rust's early return makes it report **PASSED**.
A reader of rustty's output is actively misinformed that 4 seeds × the
exhaustive budget just ran. Cost S: either `#[ignore]` it (so it reports as
ignored and is opt-in via `--ignored`), or wire a periodic/nightly gate that
sets `RUSTTY_LZ4_SLOW=1`. Recommend both.

### 4.5 **No guard on encoder byte-stream / ratio drift** — the highest-value gap

Established by measurement in §2.3: with `HASH_LOG` 12 → 11, **1,929 of 7,976**
encoded blocks diverge from upstream's byte stream and **every rustty test stays
green** (`lz4_differential_light` passes; all 13 `lz4.rs` unit tests pass; the
round-trip property is untouched because the codec remains self-consistent). The
same is true of the `advance_search` threshold and any other match-finder
heuristic.

This is precisely the failure mode the brief named — "a codec that round-trips
its own output but diverges from upstream's byte stream is a silent fidelity
break" — and it is *specified* as required coverage by the upstream doc rustty
did not port (`compress/AGENTS.md`: "When a change should not alter compressor
output, prove it: compare encoded sizes (or a sequence-count fingerprint) on the
same corpus before and after. Ratio drift is a functional change, not noise.").

Today's byte-exactness is **proven** (§2.1) but **unprotected**. Recommendation
in §5.4.

### 4.6 `compress/AGENTS.md` absent

90 lines of doctrine with no rustty counterpart: the codec priority order (ratio
on page-shaped data > decompression throughput > compression throughput), the
wasm32-freestanding constraint for libghostty-vt ("no libc, no `src/simd`"), the
differential-suite requirement (round-trip + independent walker + wrong-size
rejection + corruption, light in unit tests and exhaustive behind an env var),
the blind/wide-copy margin-argument requirement, the ratio-drift proof rule
(§4.5), the `+page-compression` / `+scrollback-compression` benchmark workflow,
and the measured performance notes (wide copies are the only SIMD that pays;
`@memcpy` beats stride loops only above ~64 bytes).

Precedent exists for per-directory AGENTS.md in rustty-vt
(`crates/rustty-vt/src/apc/glyph/AGENTS.md`), so this is a straightforward gap.
Cost S.

---

## 5. Vacuous or missing coverage

### 5.1 Upstream test hunks with **no** rustty counterpart

| Upstream test | Location | rustty |
|---|---|---|
| `test HyperlinkMap` | `benchmark/HyperlinkMap.zig` | **ABSENT** (whole file absent) |
| `test ApcParser` | `benchmark/ApcParser.zig` | **ABSENT** (whole file absent) |
| `test { refAllDecls }` | `terminal/compress.zig` | no equivalent; nearest is `rustty-bench`'s `tests::ref_all_decls` |

Everything else is fully accounted for. Test counts, oracle → rustty:
`lz4.zig` 12 (+1 import-test) → 13; `lz4_differential.zig` 2 → 2; `Page.zig`
4 → 4; `mem.zig` 3 → 3; `PageCompression.zig` 2 → 2;
`ScrollbackCompression.zig` 3 → 3. **No test-count regression in any ported
file.** The three new `ScreenClone` modes and the `TerminalStream` handler swap
added no upstream tests, so their absence produces no count delta — which is
exactly why a count gate cannot see them (§5.3).

### 5.2 Ledger rows that are stale (claim open work that shipped)

| `MAIN_PARITY.md` | Row | Reality |
|---|---|---|
| `:76` | `d0a26d146` — "missing BL-1" | **PORTED**: `pagelist/grow.rs:576,666` + verbatim test at `pagelist/tests.rs:2365` |
| `:127` | `6d5dda40d` — "missing BL-1" | **PORTED**: `rustty-inspector/src/widgets/pagelist.rs:62,413,466,2464` |
| `:134` | `0d015b2fc` — "missing BL-1" | **PORTED**: 9 provenance-tagged sites across `search/{sliding_window,active,viewport}.rs` |

These are consistent with the program's documented history of ledger rows being
wrong in both directions. Phase 3 should correct them; the substantive work is
done.

### 5.3 Guards that would pass even if the behaviour regressed

1. **`lz4_differential_exhaustive`** — passes unconditionally; `RUSTTY_LZ4_SLOW`
   is set nowhere (§4.4). *Would fail* only if that variable were set.
2. **Encoder byte-stream drift** — no guard at all; measured to admit a
   1,929/7,976 divergence silently (§4.5).
3. **`compressed_page_requires_the_maximum_useful_scratch`'s first assertion** —
   tautological restatement of `required_scratch`'s formula; cannot detect an
   overhead change (§4.2). Its second assertion (`required - 1` ⇒
   `OutputTooSmall`, source bytes unchanged) is real.
4. **`cli::tests::bench_registers_compression_actions`** (`cli.rs:293`) — real
   but narrowly scoped: it asserts only `page-compression` and
   `scrollback-compression` resolve. It passes with `hyperlink-map` and
   `apc-parser` entirely absent, and would keep passing if any *other* action
   were dropped. *Would fail* if either compression action were unregistered.
5. **`rustty-bench` is in NO gate.** `grep -rn "rustty-bench" .port/tools/gates/`
   → nothing. So all four benchmark findings (2 absent files, 2 partials) are
   invisible to the entire gate suite.
6. **G1/G1b structurally cannot see this area.** `check_test_counts.py:24` reads
   the oracle **working tree** at `/Users/gael/projects/ghostty/src/terminal`,
   which is pinned at `4749c4e93` — where `src/terminal/compress/` and all four
   new benchmarks **do not exist**. The gate therefore compares against zig = 0
   for every file in this area. Separately, it is currently **BLOCKED** locally
   (missing `zf`/`vaxis`/`libxev` dep checkouts), so it does not bind at all
   here. Neither is a compression defect, but both mean *no automated signal in
   this area comes from the count gates.*

### 5.4 Guards I verified ARE real (and what would make each fail)

| Guard | What would make it fail |
|---|---|
| `lz4_differential_light` | Weakening `LAST_LITERALS` or `MATCH_FIND_LIMIT`; narrowing either decoder fast-path margin (all four **measured** in §2.3). Not: match-finder heuristic changes. |
| `compress::lz4::tests::lz4_decompress_rejects_malformed_blocks` | Any change to the four-variant error taxonomy or to the exact-size contract. |
| `compress::page::tests::compressed_page_retained_mapping_round_trip` | Dropping any non-`memory` `Page` field from the shallow copy (it asserts restored `size`, `dirty`, grapheme, style `bold`, and hyperlink URI), or having `clone_buf` mutate the source mapping or the encoded bytes. |
| `compress::page::tests::compressed_page_can_retry_after_malformed_encoded_data` | Making a failed `restore`/`clone_buf` consume or corrupt the compressed value. |
| `pagelist::tests::pagelist_replacements_preserve_compression_continuation_and_mark_activity` | Dropping either `mark_activity()` call site, or replacing it with `reset()`. |
| `scrollback_compression_restores_cold_terminal_pages` | Making `visit_cold_pages` inspect metadata instead of restoring through `Node::page` (the second `Full` pass would then compress a different count). |
| `scrollback_compression_drains_incremental_compression_steps` | Any divergence between `Drain` and `Full` final representation (`MemoryStats` equality). |
| `page_compression_store_retains_exact_encoded_allocations` | Breaking the ring eviction/wraparound or the exactness of the stored blocks. |

### 5.5 On the known-OPEN uninitialized-bytes finding — status, and "is there more of this class?"

Per instruction I did not re-derive this. Status as of **my base commit
`b258787`**: `.port/phase-2/DEFERRALS.md` (~line 5010) now records it
**RESOLVED** by WS7 Phase E commit `76bd5a9` ("stop de-initializing the page
mapping fed to LZ4" — root-caused to `RefCountedSet::init`'s
`fill(Item::default())`, not to the read in `compress/page.rs:137`), with the
`leakcheck` gate wired as G6b by `f79a762`.

**Important caveat for whoever sequences Phase 3:** neither commit is an
ancestor of my base — `git merge-base --is-ancestor 76bd5a9 HEAD` → false,
likewise `f79a762`. And `.port/tools/gates/check_pagelist_compress_leaks.sh`
does not exist in my worktree, nor does `ci.sh` register G6b. So that work is
**in flight on a concurrent lane, not yet in `main`.** Any Phase 3 statement of
record should either wait for the merge or scope itself to the branch.

**Is there more of the same class?** Yes — one, already documented rather than
new: `DEFERRALS.md` records a **narrower residual**, reproduced by the committed
`mapinit_styled` harness scenario (aborts at mapping offset `0x17c`). Storing a
non-default `style::Style` re-poisons its own colour slots, because
`RefCountedSet::add` must place the caller's `T` with a typed write and
`style::Color` is a Rust enum whose `None`/`Palette` variants do not cover their
whole slot. The stated fix is making `style::Color` padding-free — 163 sites
across 17 files — carried with a 4-criterion `MIRI_EXCEPTIONS.md` basis and
listed for WS8. **Nothing in this range's diff introduces additional instances**:
`compress/page.rs`'s whole-mapping read is exactly what
`compress/Page.zig` does with `page.memory` (changing it would be the fidelity
break), and no other compression-area code reads a page mapping as bytes.

---

## 6. Cross-area dependencies, ordering, and recommendations

### Cross-area dependencies (all outbound from B)

| B item | Needs | Owner |
|---|---|---|
| `ApcParser.zig` benchmark | `f6f79acce` vt-stream bulk-slice APC dispatch (`apc_put_slice`) | **A** |
| `TerminalStream.zig` full-handler swap | `terminal.TerminalStream` (`stream_terminal.zig`, +1,103/−130) | **A** |
| `ScreenClone.zig` `render-locked` / `render-partial` | `RenderState::begin_update`, `Pin::mark_dirty` (`446f80f4e`, `render.zig` +913/−168) | **A** / **E** |
| `ScrollbackCompression` `--max-scrollback` → `max_scrollback_bytes` | the `Terminal.zig` scrollback bytes/lines split (`86f81fb5b`, `5b2d3b7df`, `10bc43420`, `f4c68d65e`, `65c48213b`, `1092204df`, `03d5fa268`, `a27e04e8f`) | **A** (+ **G** config) |
| `src/terminal/mem.zig` | shared with A per the brief. **I assessed the complete file** (222 lines = `DecommitMode`, `canReclaim`, `decommit`, `recommit`, 3 tests) because the whole file *is* the compression surface — there is no non-compression part for A to own separately. rustty path is `crates/rustty-vt/src/mem.rs`, not `pagelist/mem.rs`. | B (report) / A (coordinate) |
| Already satisfied by other areas | `0d015b2fc` (A), `172f15da3` (C), `6d5dda40d` (F), `0fb89f4ff` (G/E), `25e624569` (E) — all verified PORTED | — |

### Recommended ordering

1. **Now, unblocked, cheap, highest value:** the encoder byte-stream gate
   (§4.5) — see below. Then `HyperlinkMap.zig` (M, no blockers).
2. **Now, XS–S:** correct the three stale `MAIN_PARITY.md` rows (§5.2); fix or
   correct the `lz4_differential.rs` PRNG-fidelity doc-comment (§4.1); note the
   `required_scratch` 32→24 threshold shift (§4.2); `#[ignore]` the exhaustive
   differential so it stops reporting a false PASS (§4.4); port
   `compress/AGENTS.md` (§4.6); make the `mem.rs` test-simulation branch
   crate-external (§4.3).
3. **After area A lands its prerequisites:** `TerminalStream` handler swap +
   64 KiB reads; `ScreenClone`'s three render modes; `ApcParser.zig`;
   `--max-scrollback` rename.
4. **Do not reopen:** the LZ4 codec, `compress/Page.zig`, `mem.zig`'s logic, the
   PageList compression policy, and the two compression benchmarks. These are
   done, and §2.1 is the evidence.

### Primary recommendation — promote the differential into a permanent gate

I built the thing that is missing. `/tmp/b-comp/{z,rs}` contains a working
7,976-point Zig↔Rust byte-exactness differential that runs in **well under a
second** after a one-time `zig build-exe` + `rustc` (both of a single
dependency-free file). It is exactly the "encoded-size / sequence-count
fingerprint on the same corpus" that upstream's `compress/AGENTS.md` mandates,
and it catches the entire class `lz4_differential_light` provably cannot (§2.3).

Concretely, as a new `.port/tools/gates/check_lz4_byte_exact.sh`:

1. `git -C ../ghostty show origin/main:src/terminal/compress/lz4.zig` (+
   `lz4_differential.zig`, needed only to satisfy the trailing test import) into
   a temp dir; `zig build-exe harness.zig -O ReleaseSafe`.
2. `rustc -O -C debug-assertions=on --test`-free build of a harness that
   `#[path]`-includes `crates/rustty-vt/src/compress/lz4.rs` verbatim.
3. Compare the two outputs; non-empty diff ⇒ FAIL, printing the diverging
   `(family, size)` rows.
4. Self-test arm proving the FAIL path is reachable (the program has been burned
   by unreachable failure arms): patch `HASH_LOG` 12 → 11 in the temp copy and
   require the gate to exit non-zero with **1,929** diverging rows.

Notes for the implementer: the codec compiles cleanly under the locally
installed **Zig 0.15.2** despite upstream targeting 0.16 (verified), so the gate
does not need a 0.16 toolchain. It must remain read-only against the oracle
(`git show`, never `checkout`). Phase 3 could reasonably register it as a
PROGRESS gate alongside D3 (derived-constant parity), whose subject matter it
extends from constants to emitted bytes.

Secondary recommendation: add `rustty-bench` to *some* gate. Right now it is in
none, which is why four benchmark gaps in this area produced zero automated
signal.

### Open questions

1. **Is byte-exactness with upstream's LZ4 a stated port requirement, or only
   round-trip correctness?** It is not written down anywhere I could find. My
   view: it should be stated explicitly, because §2.1 shows it currently *holds*
   and §4.5 shows it is one heuristic edit from silently breaking. The gate
   above is cheap enough that there is no reason to accept less. (This is also
   the only sane way to keep rustty's blocks consumable by the optimized
   external LZ4 decoders upstream deliberately stays compatible with.)
2. **Do the four new/changed benchmarks belong in Phase 3's completion bar at
   all, or in a perf lane?** They carry no product behaviour. But two are
   *wholly absent files* in the diff, so a "365 commits closed" claim that
   leaves them out is not honest. Recommend: `HyperlinkMap` now (unblocked);
   the other three explicitly deferred **with rows**, blocked on named area-A
   prerequisites, rather than silently omitted.
3. **Should `cfg(test)`-gated simulation branches be audited port-wide?** §4.3
   is unlikely to be the only instance of Zig's `builtin.is_test` (module-wide)
   being ported to Rust's `cfg(test)` (crate-local). A repo-wide grep for
   `cfg!(test)` in library crates whose behaviour it *changes* (as opposed to
   test-only helpers) would be a cheap, targeted sweep. Out of scope for B.

---

## 7. Coverage statement — exhaustive vs sampled

**Read in full, both sides, hunk by hunk (exhaustive):**

- `compress/lz4.zig` (891) ↔ `compress/lz4.rs` (999)
- `compress/lz4_differential.zig` (493) ↔ `compress/lz4_differential.rs` (609)
- `compress/Page.zig` (398) ↔ `compress/page.rs` (476)
- `compress.zig` (15) ↔ `compress.rs` (18)
- `compress/AGENTS.md` (90) — read in full; confirmed absent in rustty
- `terminal/mem.zig` (222) ↔ `src/mem.rs` (338) — complete file both sides
- `benchmark/PageCompression.zig` (521) ↔ `page_compression.rs` (929)
- `benchmark/ScrollbackCompression.zig` (344) ↔ `scrollback_compression.rs` (612)
- The **complete** `4749c4e93..origin/main` diff for all 10 benchmark-infra
  files (`Benchmark`, `CApi`, `CodepointWidth`, `GraphemeBreak`, `IsSymbol`,
  `OscParser`, `TerminalParser`, `cli`, `main`, `options`)
- The **complete** diff of `benchmark/ScreenClone.zig` and
  `benchmark/TerminalStream.zig` (upstream side)
- Upstream `PageList.zig`'s entire compression surface (`CompressionScratch`,
  `CompressionIterator`, `compress`, `compressIncremental`, `compressFull`,
  `compressPage`, `MemoryStats`, `memoryStats`, `compressPage_tw`) ↔
  `pagelist/compress.rs` (688)
- `git log 4749c4e93..origin/main -- src/terminal/compress src/benchmark` and a
  message-grep for every compression/madvise/scrollback/lz4 commit in the range
  (26 commits) — each classified

**Sampled, not read line by line (stated as such):**

- `benchmark/HyperlinkMap.zig` and `benchmark/ApcParser.zig` — read the module
  docs, `Options`, `Mode`, and handler surface; did **not** read every line.
  Both are wholly absent from rustty, so line-level reading would add cost, not
  information. My §3.1/§3.2 hunk lists are therefore *surface-complete, not
  line-complete*.
- rustty's `screen_clone.rs` and `terminal_stream.rs` — grepped their `Mode` /
  `Options` / handler / buffer-size surface to establish what is missing; did
  **not** read either file in full.
- The ~30 compression **tests** inside upstream `PageList.zig` — I confirmed the
  policy code and spot-checked specific tests (`d0a26d146`'s, and the
  `memoryStats` assertions). A test-for-test audit of `PageList.zig`'s body
  belongs to **area A**.
- `rustty-vt`'s `pagelist/tests.rs` — searched for specific compression tests;
  did not enumerate the whole file.

**Verification executed (not inferred):**

| Check | Result |
|---|---|
| Zig↔Rust LZ4 differential, 4 phases | **7,976 / 7,976 identical, 0 divergences** |
| Rust round-trips inside the differential | 4,376 / 4,376 clean |
| Panics under runtime safety (both sides, 3,600 corrupted decodes) | 0 |
| Fault-injection matrix vs `lz4_differential_light` | 7 faults: 4 caught, 3 missed (all 3 characterized) |
| Byte-stream divergence proof for a missed fault | `HASH_LOG` 12→11 ⇒ **1,929 / 7,976** rows diverge, suite green |
| `cargo check -p rustty-vt` | clean |
| `cargo test -p rustty-vt --lib --release compress::` | 19 / 19 pass |
| `cargo test -p rustty-bench --lib --release` | 14 / 14 pass |
| `required_scratch` overhead probe | 24 (vs upstream 32) |
| `RUSTTY_LZ4_SLOW` set anywhere in repo | no |
| `rustty-bench` referenced in any gate | no |
| `76bd5a9` / `f79a762` ancestors of base | no (concurrent lane) |

**Not done, deliberately:** no production code modified; no sub-agents; no
`checkout` in the oracle; `no_homebrew_gettext_gate.sh` not run; the full
`ci.sh` suite not run (out of scope and expensive — G1b is BLOCKED locally
anyway, §5.3).
