# Diagnosis — Zig-winning benchmarks (perf-new7)

Phase-1 evidence (systematic-debugging): CPU profiles via macOS `sample` (8s,
release-fast rustty-bench vs ReleaseFast ghostty-bench @ 4540d499a, same
corpora as RESULTS.md). Raw samples: /tmp/{rust,zig}_{encode,fmtvt,rows,churn}_sample.txt.
Code-archaeology findings from the 4 diagnosis agents get merged in below as
they land.

## 1. terminal-snapshot.encode (+277-298%, 3.9×) — PROFILED

Rust self-time (7.7k in-process samples):
- `_platform_memmove` 2394 (31%) + `_platform_memset` 1056 (14%) + `DYLD-STUB$$memcpy` 568 (7%) → **~52% of ALL time in bulk memory ops**
- `snapshot::grid::encode_narrow_step` 1557 (20%) + `snapshot::grid::encode` 832 (11%) → 31% in the per-cell grid encode loop
- `crc32c::Crc32c::update` 271 (3.5%) — software CRC, modest
- `AllocatingWriter::write_all` 49

Zig self-time: `terminal.snapshot.page.encode` 351, CRC `Io.Writer.Hashing(crc32c).drain` 133, memmove **19**.

READ: Zig's encode has essentially zero bulk-copy traffic; Rust's encode is
half memcpy/memset. Hypothesis (to be confirmed vs upstream commits): rustty
predates c1a61fddd "borrow fully buffered record payloads" (staging copies) and
the vectorized/batched grid encode commits (973f619a2, ed0f54fb8, 9e3019f19),
and something (buffer prep) memsets large regions per record.

## 2. terminal-formatter.format-vt (+84-86%) — PROFILED

Same hot function on both sides — the shape matches, the loop is just ~2× slower:
- Rust: `PageFormatter::write_cell_run` 199/365 (55%), `format_with_state` 49,
  `encode_codepoint` 48, memmove 27, `PrintDecimal::print_decimal` 17,
  `VtFormatter::{format,append_color}` 18, **`core::str::from_utf8` 7 (Rust-only —
  UTF-8 validation in the emit path; Zig writes raw bytes)**
- Zig: `writeCellRun__anon` 3316/~5600 (59%), `formatWithState` 1071,
  `VTFormatter.format` 385, memmove 532.

READ: gap is inside write_cell_run per-unit cost: candidate causes = per-cell
codepoint encoding (`encode_codepoint` + `from_utf8` validation) where Zig's
comptime-specialized writer writes bytes directly; possibly dyn-Writer dispatch
per write vs Zig's `__anon` monomorphized writer.

## 3. terminal-resize.rows (+33%) — PROFILED

- Rust: **`Page::layout` 1964 (top self-time!)**, `resize_without_reflow` 1778,
  `Screen::resize_tracked` 408, `manual_style_update` 334,
  `RefCountedSet::add_context` 283, `Terminal::resize` 235,
  `pagelist::resize` 221, **`Style::hash` 221 (standalone)**,
  `Limits::min_max_size` 200, `cursor_reload` 120
- Zig: `PageList.resizeWithoutReflow` 1381, **`page.Capacity.adjust` 713**
  (Zig's analogue of layout math), `Terminal.resize` 537, `PageList.resize` 286,
  `manualStyleUpdate` 246, `lookupContext` 200, `addContext` 108.

READ: Rust burns ~2160 samples in pure layout/limits arithmetic
(`Page::layout` + `min_max_size`) vs Zig's 713 in `Capacity.adjust` —
either called far more often (recomputed redundantly) or much slower per call
(div/align math, not inlined). Second: `Style::hash` not inlined into set ops.

## 4. hyperlink-map.churn (+37-39%) — PROFILED

- Rust (~5k in-process samples): `HashMapUnmanaged::remove_by_ptr_context` 1749
  (35%), **Wyhash `finalize` 696 + `update` 581 + `small_key` 493 + `Hasher::write` 9
  = 1779 (35%!) as separate un-inlined symbols**, `get_or_put_context_adapted` 553,
  `_platform_memmove` 467 (9%), `get_entry_adapted` 161, page get/set/clear ~263.
- Zig: `page.Page.clearHyperlink` 1088, `getOrPut` (AutoContext(Offset(Cell)))
  544 — hash fully inlined, **zero standalone hash samples, zero memmove**.

READ: the key is a 4-byte cell offset; Zig hashes it with an inlined
specialized autoHash. Rust routes it through a generic streaming Wyhash
(update/finalize call chain) that fails to inline — ~35% of churn time is
hash overhead Zig doesn't pay. Plus memmove traffic on the Rust side (find
source — possibly in remove path).

## Still to fold in
- Agent reports: port vintage + upstream-commit presence tables for all 4 areas
  (snapshot, formatter, hyperlink/page-map, resize) + code-level root causes.
- terminal-snapshot.decode (+50%) — likely shares causes with encode; confirm
  after encode diagnosis.
- terminal-formatter.format-plain (+21%) — likely milder version of format-vt
  causes (no SGR emission).
- scrollback-compression (+3-12%) — borderline noise; re-check after other
  fixes land.

---

# Fixes applied (2026-08-28, this session)

All four agent diagnoses agreed: nothing was missing from upstream — every gap
was the port flattening Zig COMPTIME machinery into runtime code. Fixes, each
measured individually (loops-sweep ns/op for hyperlink, min-of-N wall-clock
for the rest):

## Hyperlink map (churn 6.1x -> 1.26x, lookup 7.5x -> 1.12x, floor-corrected)
1. `rustty-zigstd/src/wyhash.rs`: `#[inline]` on the whole public API
   (`init`/`update`/`finalize`/`hash` + the `Hasher` impl) — cross-crate
   callers can't inline without LTO (banned). 6.1x -> 3.9x.
2. `rustty-vt/src/hash_map.rs` `AutoContext::hash`: the reference's
   `hasUniqueRepresentation` split ported for real — small fixed-width keys
   take the ONE-SHOT `Wyhash::hash` over the key bytes (`SmallKeyBytes`
   reassembles them; digest unchanged) with a CONST length
   (`size_of::<K>()`, the Rust spelling of `asBytes`'s comptime length; a
   runtime length left the <=16-byte branch unfoldable). 3.9x -> 1.5x.
3. `#[inline]` on `Page::{get_row_and_cell, lookup_hyperlink}` and the map's
   `get_entry_adapted`/`get_context`/`get_adapted` — the bench crate (and the
   C API) call these cross-crate; Zig's single-CU build inlines them freely.
   -> churn 1.26x / lookup 1.12x.

## Snapshot codec (encode +295% -> +16%, decode +50% -> +14%)
4. `snapshot/grid.rs`: the encoded width is now a const-generic `SIZE` on
   `encode_narrow_{cells,into,step}` / `widen_{cells,step}` / the
   `truncate/extend/widen_value` helpers, dispatched by a runtime-width
   match (the Rust spelling of the reference's `inline .one, .two, .four =>
   |width|`). The runtime width had made every per-cell store a
   dynamic-length `copy_from_slice` — one libc memcpy call PER CELL.
   +295% -> +85%.
5. The 4 KiB bulk row chunk hoisted out of the row loop (was a per-row
   memset — ~36% of encode; the reference writes into the writer's spare
   capacity and never zeroes). +85% -> +28%.
6. `encode_narrow_step` packs the four truncated values into one
   `4*SIZE`-byte integer store (the scalar spelling of the reference's
   two-lane shift + byte shuffle). +28% -> +16%.

## Terminal resize (rows +34% -> ~+6-10%, cols/both +26-29% -> ~+20-25%)
7. `pagelist/mod.rs` `std_size()`: cached in a `OnceLock` (the reference's
   comptime `PagePool.item_size`); recomputing the ~90-line `Page::layout`
   here put a third full layout on every resize.
8. `#[inline]` on `Page::layout` (lets the ~300-byte `Layout` dead-strip to
   the fields each caller reads, as Zig gets from comptime inlining) and on
   `Style::hash`/`hash_int_u64` (reference force-inlines both).
9. TRIED AND REVERTED: converting the hot files' bare `assert!` (ports of
   `quirks.inlineAssert`, elided in ReleaseFast) to `debug_assert!`. Net
   effect within noise on cols and reproducibly NEGATIVE on rows — Zig's
   ReleaseFast assert is `if (!ok) unreachable`, which KEEPS the optimizer
   hint; `debug_assert!` in release provides neither the check nor the hint,
   and some asserts were load-bearing for bounds-check elimination. The
   remaining cols/both ~+20% is bounds-check-shaped (reflow_row inner loop);
   candidates in the resize agent report (unproven).

## Formatter (format-vt +85% -> +23%, format-plain +21% -> +10%)
10. `style.rs` `VtFormatter::format` split into a byte-core `format_into`;
    `format_style_open`'s Vt arm emits bytes straight to the `io::Write`
    sink — kills the per-style-change `core::str::from_utf8` validation
    scan and the non-inlinable `IoToFmt` adapter hop.
11. `#[inline]` on `append_color`/`append_color_rgb`/`print_decimal` (+ its
    impls); reference inlines the whole SGR chain.
12. The 512-byte run buffer hoisted from `write_cell_run` (zeroed twice per
    style boundary) to `format_with_state_emit`, threaded as a parameter
    (the reference's is `undefined` in-frame).
13. `write_cell_run` and `encode_codepoint` monomorphized over a
    const-generic `EMIT` discriminant (the reference's `comptime emit:
    Format`), dispatched once per run; `encode_codepoint` is
    `#[inline(always)]` (plain `#[inline]` lost to the cost model).
    Roundtrip mode verifies byte-equal replay.

## Cross-check
- Full rustty-vt suite: 2968+ tests green post-change (workspace suite +
  gates re-verified at the end of the session; see RESULTS.md for the final
  19-config table).
- The floor-correction discovery (hyperlink agent): rustty's process floor is
  ~3 ms LOWER than ghostty's, so wall-clock deltas on short benches
  UNDERSTATE rust deficits and fake rust wins (`hyperlink-map.lookup` "won"
  at -15% while being 7.5x slower per op). Re-read the RESULTS.md short-bench
  rows (`apc-parser`, `formatter.noop`) with that lens.
