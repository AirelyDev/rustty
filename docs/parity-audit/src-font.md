# Parity audit — src/font

Seventeen shards (84–100) covered the whole of `src/font` at oracle pin `20abdb50a`: 40 oracle units read in full (resolver/collection, deferred face, glyph, metrics, shared grid + grid set, backend, discovery, embedded, face selector, glyf rasterizer + its PNG KAT, library, main, the 2774-line nerd-font attribute table, opentype (7 files), shaper (cache, coretext, feature, harfbuzz, noop, run, web_canvas), sprite Face/canvas and all 10 `sprite/draw` modules), plus `src/input/Binding.zig` which shard 100 carried as a spill-over unit. **31,229 lines read**, with zero units left partially read.

51 findings survived to the report: **32 CONFIRMED, 14 DISPUTED, 5 REFUTED**. By corrected severity across the 46 non-refuted findings: **0 high, 8 medium, 38 low**. Roughly a third of the low tier is one systemic issue — `rustty-font-shaper` and `rustty-font` wire no logging facility at all, so every `log.warn/info/debug/err` in the oracle survives only as a `// log.…` comment.

The single most important gap is **S97-F1**: the sprite golden `U+2500...U+25FF-9x17+1.png` was regenerated from rustty's own render instead of copied from the oracle, and it differs from the oracle testdata at one pixel (x=133, y=381: 79 vs 95) — a real render divergence in `U+25FA` at 9×17 thickness 1 that the ported KAT structurally cannot detect, because its reference was replaced.

## Coverage

| Oracle unit (path:lines)                                                   | rustty counterpart(s)                                                                                                                       | Status      | Note                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| -------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------- | ----------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `src/font/CodepointResolver.zig` [1,562]                                   | `crates/rustty-font-shaper/src/codepoint_resolver.rs`                                                                                       | ported      | Fields/defaults, 7-step `getIndex`, codepoint override + descriptor cache (incl. null-caching on FontNotFound), presentation, renderGlyph, StyleStatus, DescriptorCache. All 3 tests ported with identical assertions.                                                                                                                                                                                                                                  |
| `src/font/Collection.zig` [1,1524]                                         | `crates/rustty-font-shaper/src/collection.rs`                                                                                               | ported      | add/addDeferred/getFace/completeStyles/synthetic bold+italic/setSize/scaleFactor/updateMetrics, `Index` u16 packing (sprite = 8191), `StyleArray` as `Vec<Box<>>` for pointer stability. All tests ported; the face-metrics test is `#[ignore]` mirroring the oracle's `SkipZigTest`.                                                                                                                                                                   |
| `src/font/DeferredFace.zig` [1,531]                                        | `crates/rustty-font-shaper/src/deferred_face.rs`; `crates/rustty-wasm/src/lib.rs`; `crates/rustty-web-canvas/src/lib.rs`                    | ported      | All four backend payloads behind cfg; wasm C-ABI behind `ghostty-wasm-abi`. `loadCoreTextFreetype` is ported LIVE for `-Dfont-backend=coretext_freetype` (with the `.ttc` face-index fix UD280); the `.freetype` no-payload arms too. CORRECTION (S-14, N266): this row said "`loadCoreTextFreetype` is not-applicable (rustty's `BACKEND` const can never be `CoretextFreetype`)" — true only while `-Dfont-backend` was folded to the per-OS default. |
| `src/font/Glyph.zig` [1,630]                                               | `crates/rustty-font/src/glyph.rs`                                                                                                           | ported      | Glyph/Size/DesignMetrics/RenderOptions/Constraint and all scale/align math match line by line; the `Constraints` test keeps all 17 assertions.                                                                                                                                                                                                                                                                                                          |
| `src/font/Metrics.zig` [1,818]                                             | `crates/rustty-font/src/metrics.rs`; `crates/rustty-config/src/config_struct.rs:8610-8626`                                                  | ported      | Metrics/Minimums/FaceMetrics/calc/apply/addFloatToInt/ModifierSet/Modifier/Key(17). All 9 tests ported.                                                                                                                                                                                                                                                                                                                                                 |
| `src/font/SharedGrid.zig` [1,664]                                          | `crates/rustty-font-shaper/src/shared_grid.rs`                                                                                              | ported      | init (3 tripwire points), reloadMetrics, getIndex rollback, renderGlyph atlas-grow retry, `GlyphKey.Packed` bit layout. 3/3 tests.                                                                                                                                                                                                                                                                                                                      |
| `src/font/SharedGridSet.zig` [1,853]                                       | `crates/rustty-font-shaper/src/shared_grid_set.rs`; `crates/rustty-core/src/surface.rs:1102-1203`; `crates/rustty-cli/src/show_face.rs:620` | ported      | Same face list in the same order (per-style discovery + variations retry, completeStyles, variable wght 700 arms, nerd symbols, macOS Apple Color Emoji fast path, non-macOS embedded emoji). 4/4 tests + 1 extra.                                                                                                                                                                                                                                      |
| `src/font/backend.zig` [1,128]                                             | `crates/rustty-build-config/src/font.rs`; `crates/rustty-font-shaper/src/backend.rs`                                                        | ported      | 8 variants, `default()` per target, `hasFreetype/Coretext/Fontconfig/Harfbuzz` arm-for-arm. Oracle has no tests.                                                                                                                                                                                                                                                                                                                                        |
| `src/font/discovery.zig` [1,1416]                                          | `crates/rustty-font-shaper/src/discovery.rs`                                                                                                | ported      | Descriptor (hash/hashcode/toFcPattern/toCoreTextDescriptor), Fontconfig/CoreText/Windows discovery + iterators + Score, all 8 tests.                                                                                                                                                                                                                                                                                                                    |
| `src/font/embedded.zig` [1,55]                                             | `crates/rustty-font/src/embedded.rs`                                                                                                        | **partial** | 18 faces embedded via `include_bytes!`; `bold`/`italic`/`bold_italic` left as a PORT NOTE deferral (S87-F3).                                                                                                                                                                                                                                                                                                                                            |
| `src/font/face.zig` [1,112]                                                | `crates/rustty-font-shaper/src/face/mod.rs`; `crates/rustty-apprt/src/gtk/class/surface.rs`                                                 | ported      | Face selector, `default_dpi`, FreetypeLoadFlags, Options/DesiredSize/Variation + both Variation.Id tests. GTK boxed type relocated (S87-F4).                                                                                                                                                                                                                                                                                                            |
| `src/font/glyf_rasterize.zig` [1,827]                                      | `crates/rustty-font/src/glyf_rasterize.rs` (+ `glyf_rasterize/glyf_rasterize_png_test.rs`)                                                  | ported      | Bitmap/rasterize/Bounds/Placement/appendContourPath/transformPoint and all 13 named tests with identical assertion sets.                                                                                                                                                                                                                                                                                                                                |
| `src/font/glyf_rasterize_png_test.zig` [1,303]                             | `crates/rustty-font/src/glyf_rasterize/glyf_rasterize_png_test.rs`; `crates/rustty-font/res/glyf_rasterize.png`                             | ported      | All 5 base64 payloads byte-identical; reference PNG re-encoded but decodes to an identical 200×80 pixel plane.                                                                                                                                                                                                                                                                                                                                          |
| `src/font/library.zig` [1,64]                                              | `crates/rustty-font-shaper/src/library.rs`                                                                                                  | ported      | Freetype/Noop libraries; the `switch (options.backend)` collapses to a cfg because every reachable rustty configuration uses the target default.                                                                                                                                                                                                                                                                                                        |
| `src/font/main.zig` [1,83]                                                 | `crates/rustty-font-shaper/src/lib.rs`; `crates/rustty-font/src/lib.rs`; `crates/rustty-wasm/src/lib.rs:40-80`                              | ported      | All 25 pub re-exports split across the D1 leaf and shaper tier; Style/Presentation/sprite_index/default_fallback_adjustment/options.backend all match.                                                                                                                                                                                                                                                                                                  |
| `src/font/nerd_font_attributes.zig` [1,2774]                               | `crates/rustty-font/src/nerd_font_attributes.rs`                                                                                            | ported      | Full structural diff: 276 arms in identical order, 10410 codepoints on both sides, byte-identical attribute values; zero unclassified oracle lines.                                                                                                                                                                                                                                                                                                     |
| `src/font/opentype.zig` [1,19]                                             | `crates/rustty-font/src/opentype/mod.rs`                                                                                                    | ported      | Submodule root; capitalized aliases reached as Rust paths. `refAllDecls` not-applicable.                                                                                                                                                                                                                                                                                                                                                                |
| `src/font/opentype/glyf.zig`                                               | `crates/rustty-font/src/opentype/glyf.rs`                                                                                                   | ported      | Glyf/Outline/Entry/Header/SimpleFlags + all 14 tests; extra `decode_limited` mirrors the LimitedAllocator semantics.                                                                                                                                                                                                                                                                                                                                    |
| `src/font/opentype/head.zig`                                               | `crates/rustty-font/src/opentype/head.rs`                                                                                                   | ported      | 18 fields in on-disk order, EndOfStream on underflow, full-struct test equality.                                                                                                                                                                                                                                                                                                                                                                        |
| `src/font/opentype/hhea.zig`                                               | `crates/rustty-font/src/opentype/hhea.rs`                                                                                                   | ported      | 18 fields incl. reserved; full-struct test equality.                                                                                                                                                                                                                                                                                                                                                                                                    |
| `src/font/opentype/os2.zig`                                                | `crates/rustty-font/src/opentype/os2.rs`                                                                                                    | ported      | FSSelection bits, version-gated tails, 41-field test. Versioned extern structs collapsed into `init` (S92-F1 on error ordering).                                                                                                                                                                                                                                                                                                                        |
| `src/font/opentype/post.zig`                                               | `crates/rustty-font/src/opentype/post.rs`                                                                                                   | ported      | 9 fields, Version16Dot16 decode, identical test.                                                                                                                                                                                                                                                                                                                                                                                                        |
| `src/font/opentype/sfnt.zig`                                               | `crates/rustty-font/src/opentype/sfnt.rs`                                                                                                   | ported      | Type aliases, FixedPoint (same half-away-from-zero rounding), SFNT.init/getTable, all 3 tests. Missing the three `format` methods (S92-F4).                                                                                                                                                                                                                                                                                                             |
| `src/font/opentype/svg.zig`                                                | `crates/rustty-font/src/opentype/svg.rs`                                                                                                    | ported      | init/hasGlyph/compareGlyphId/glyphRange + both tests; two added bounds guards (S92-F2, S92-F3).                                                                                                                                                                                                                                                                                                                                                         |
| `src/font/shape.zig` [1,102]                                               | `crates/rustty-font-shaper/src/shape.rs`                                                                                                    | ported      | Shaper selector via cfg, Cell/Options/RunOptions/apply_break_config, module test.                                                                                                                                                                                                                                                                                                                                                                       |
| `src/font/sprite.zig` [1,45]                                               | `crates/rustty-font/src/sprite/mod.rs`                                                                                                      | ported      | `Sprite` repr(u32), 11 variants in order, START = 1<<21, END = u32::MAX, enum test.                                                                                                                                                                                                                                                                                                                                                                     |
| `src/font/face/coretext.zig` [1,1163]                                      | `crates/rustty-font-shaper/src/face/coretext.rs`                                                                                            | ported      | Face lifecycle, synthetic bold/italic, set_variations, render_glyph and get_metrics line-for-line (same constants, sbix quantization, CTM, metric cascades), ColorState, all 8 tests.                                                                                                                                                                                                                                                                   |
| `src/font/face/freetype.zig` [1,1440]                                      | `crates/rustty-font-shaper/src/face/freetype.rs`                                                                                            | ported      | All methods, constants and error cases; all 9 oracle tests + 3 additive stb-resize KATs.                                                                                                                                                                                                                                                                                                                                                                |
| `src/font/face/web_canvas.zig` [1,561]                                     | `crates/rustty-font-shaper/src/face/web_canvas.rs`; `crates/rustty-os/src/wasm/host_owned.rs`                                               | **partial** | Ported live under cfg(wasm32) with a `rustty_wc_*` extern-C FFI. The oracle file does not compile at the pin (uses a non-existent Atlas `.rgba` format and an under-specified `Metrics` literal), so five divergences are deliberate (S91-F1..F4 + notes).                                                                                                                                                                                              |
| `src/font/shaper/Cache.zig` [1,106]                                        | `crates/rustty-font-shaper/src/shaper/cache.rs`; `crates/rustty-datastruct/src/cache_table.rs`                                              | ported      | Same 256×8 CacheTable, get/put/clear with the free routed through the `evicted` hook; the `Cache` test ported with both assertions.                                                                                                                                                                                                                                                                                                                     |
| `src/font/shaper/coretext.zig` [1,2679]                                    | `crates/rustty-font-shaper/src/shaper/coretext.rs` (+ `coretext_tests.rs`)                                                                  | ported      | Shaper/RunState/Offset/Codepoint, makeFeaturesDict, full `shape()` incl. cluster-reset heuristic and non-LTR stable sort, getFont caching, RunIteratorHook. All 27 test blocks with identical assertions.                                                                                                                                                                                                                                               |
| `src/font/shaper/feature.zig` [1,353]                                      | `crates/rustty-font-shaper/src/shaper/feature.rs`; `shape.rs`                                                                               | ported      | 7-state parser arm-for-arm, toString/format, FeatureList, `default_features`, both tests (27 + 1 assertions).                                                                                                                                                                                                                                                                                                                                           |
| `src/font/shaper/harfbuzz.zig` [1,2236]                                    | `crates/rustty-font-shaper/src/shaper/harfbuzz.rs` (+ `harfbuzz_tests.rs`, `hb.rs`)                                                         | ported      | init/deinit/endFrame/runIterator/shape line-for-line (26.6 rounding, ligature cell_offset heuristic, feature offsets), all 25 test blocks. Only `debugPositions` is comment-only (S96-F1, refuted).                                                                                                                                                                                                                                                     |
| `src/font/shaper/noop.zig` [1,143]                                         | `crates/rustty-font-shaper/src/shaper/noop.rs`                                                                                              | ported      | Shaper/RunState/Codepoint/hook all match; `refAllDecls` ported as a real test.                                                                                                                                                                                                                                                                                                                                                                          |
| `src/font/shaper/run.zig` [1,396]                                          | `crates/rustty-font-shaper/src/shaper/run.rs`                                                                                               | ported      | `RunIterator.next` break/skip/fallback chain, addCodepoint, indexForCell, comparableStyle all line-for-line; hook abstracted as a trait.                                                                                                                                                                                                                                                                                                                |
| `src/font/shaper/web_canvas.zig` [1,305]                                   | `crates/rustty-font-shaper/src/shaper/web_canvas.rs`                                                                                        | **partial** | Shaper + hook fully ported under cfg(wasm32); the oracle body references deleted APIs (`run.group.group.faceFromIndex`, 3-arg `Terminal.init`, 4-arg `runIterator`), so `shaper_test` was modernized (S97-F2).                                                                                                                                                                                                                                          |
| `src/font/sprite/Face.zig` [1,668]                                         | `crates/rustty-font/src/sprite/face.rs`; `sprite/mod.rs`                                                                                    | ported      | The comptime-reflected `ranges` table hand-materialized (54 draw fns, verified identical set/order/no-overlap), getDrawFn, renderGlyph, testDiffAtlas/testDrawRanges, both tests.                                                                                                                                                                                                                                                                       |
| `src/font/sprite/canvas.zig` [1,445]                                       | `crates/rustty-font/src/sprite/canvas.rs`                                                                                                   | ported      | Point/Line/Box/Rect/Triangle/Quad/Color/Canvas, writeAtlas, trim, all path/stroke/fill/invert/flip helpers. No oracle tests; rustty adds 3.                                                                                                                                                                                                                                                                                                             |
| `src/font/sprite/draw/block.zig`                                           | `crates/rustty-font/src/sprite/draw/block.rs`                                                                                               | ported      | All 32 arms 0x2580–0x259F, block/blockShade/fullBlockShade/quadrant identical.                                                                                                                                                                                                                                                                                                                                                                          |
| `src/font/sprite/draw/box.zig`                                             | `crates/rustty-font/src/sprite/draw/box.rs`                                                                                                 | ported      | All 128 arms 0x2500–0x257F verified one-by-one, linesChar, diagonals, arc, dashHorizontal/Vertical.                                                                                                                                                                                                                                                                                                                                                     |
| `src/font/sprite/draw/braille.zig`                                         | `crates/rustty-font/src/sprite/draw/braille.rs`                                                                                             | ported      | Bit layout, all 5 px_left stages, 3 asserts, 8 dot boxes at identical coords.                                                                                                                                                                                                                                                                                                                                                                           |
| `src/font/sprite/draw/branch.zig`                                          | `crates/rustty-font/src/sprite/draw/branch.rs`                                                                                              | ported      | All 62 arms 0xF5D0–0xF60D, branchNode geometry, fadingLine gradients.                                                                                                                                                                                                                                                                                                                                                                                   |
| `src/font/sprite/draw/common.zig`                                          | `crates/rustty-font/src/sprite/draw/common.rs`                                                                                              | ported      | Fractions, Thickness, Shade, Quads/Corner/Edge, Alignment (22 consts), Fraction (29 variants + tables), fill/vline/hline.                                                                                                                                                                                                                                                                                                                               |
| `src/font/sprite/draw/geometric_shapes.zig`                                | `crates/rustty-font/src/sprite/draw/geometric_shapes.rs`                                                                                    | ported      | draw25E2_25E5/25F8_25FA/25FF, identical corner vertex tables and path/stroke options.                                                                                                                                                                                                                                                                                                                                                                   |
| `src/font/sprite/draw/powerline.zig`                                       | `crates/rustty-font/src/sprite/draw/powerline.rs`                                                                                           | ported      | All 18 draw fns with identical geometry, arc coefficient, caps, flips.                                                                                                                                                                                                                                                                                                                                                                                  |
| `src/font/sprite/draw/special.zig`                                         | `crates/rustty-font/src/sprite/draw/special.rs`                                                                                             | ported      | All 11 underline/strikethrough/overline/cursor fns with identical clamps and dot/dash/curly math.                                                                                                                                                                                                                                                                                                                                                       |
| `src/font/sprite/draw/symbols_for_legacy_computing.zig`                    | `crates/rustty-font/src/sprite/draw/symbols_for_legacy_computing.rs`                                                                        | ported      | All 44 mosaic patterns verified byte-for-byte, sextants, all 18 pub fns and helpers.                                                                                                                                                                                                                                                                                                                                                                    |
| `src/font/sprite/draw/symbols_for_legacy_computing_supplement.zig` [1,633] | `crates/rustty-font/src/sprite/draw/symbols_for_legacy_computing_supplement.rs`; `octants.txt`                                              | ported      | Every fn/arm/constant 1:1; `octants.txt` byte-identical (diff clean); comptime parser reproduced as a const fn.                                                                                                                                                                                                                                                                                                                                         |
| `src/input/Binding.zig` [1,1675] (spill-over unit, shard 100)              | `crates/rustty-input/src/binding.rs`                                                                                                        | ported      | Flags/cval, Parser + SequenceIterator, lessThan, the 89-variant Action union, all payload enums, parse dispatch, Scope/Scoped, format, clone/hash. `Action.getGObjectType` deferred (S100-F2); numeric parsing differs (S100-F1).                                                                                                                                                                                                                       |

**Partially-read units: none.** Every unit listed in every shard's `units_fully_read` was read end to end, and no shard reported a `units_partial` entry. Three _counterparts_ are marked `partial` above (`embedded.zig`, `face/web_canvas.zig`, `shaper/web_canvas.zig`) — that is a port-completeness classification, not a coverage gap; each has an associated finding. One candidate (`symbols_for_legacy_computing_supplement.zig` listed under shard 99) was correctly classified `not-applicable` there because it belongs to shard 100.

## Findings

### High severity

None.

### Medium severity

#### S85-F1 — Modifier::parse rejects `_` digit separators (and hex floats) that Zig's std.fmt.parseInt/parseFloat accept

- **Status: CONFIRMED** (both lenses verified) · kind: behavioral-difference · severity: medium
- Oracle: `src/font/Metrics.zig:464-484`
- rustty: `crates/rustty-font/src/metrics.rs:700-727`

The oracle parses the absolute arm with `std.fmt.parseInt(i32, input, 10)` and the percent arm with `std.fmt.parseFloat(f64, ...)`. Zig 0.16 std `parseInt` is documented "Ignores '_' character in `buf`" (leading/trailing `_` → InvalidCharacter, interior `_` skipped), and `parseFloat` also accepts interior underscores and hex-float syntax. The port uses Rust `str::parse::<i32>()` / `parse::<f64>()`, which reject any `_` and hex floats. So a config value such as `adjust-cell-height = 1_0` (oracle: absolute 10) or `adjust-cell-width = 2_0%` (oracle: percent 1.2) is accepted by Ghostty but returns `Error::InvalidFormat` in rustty.

_Evidence:_ `Metrics.zig:467-483` `std.fmt.parseFloat(f64, input[0..len-1])` / `std.fmt.parseInt(i32, input, 10)`; `metrics.rs:706-711`, `:720-725` `.parse::<f64>()` / `.parse::<i32>()`; Zig std `fmt.zig:315` "Ignores '_' character in `buf`", `:439-441` implementation.

_Suggested fix:_ Port Zig's parseInt/parseFloat underscore rules (reject leading/trailing/double `_`, strip interior `_` between digits) before delegating to Rust's parsers, or add a faithful shim in `rustty-util`.

#### S85-F2 — CoreText familyName lacks the `orelse return "unknown"` NULL fallback

- **Status: CONFIRMED** · kind: missing-branch · severity: medium
- Oracle: `src/font/DeferredFace.zig:151-159`
- rustty: `crates/rustty-font-shaper/src/deferred_face.rs:280-299`

The oracle does `ct.font.copyAttribute(.family_name) orelse return "unknown"`. The port calls objc2's `ct.font.family_name()`, generated as non-null with `.expect("function was marked as returning non-null, but actually returned NULL")` (objc2-core-text-0.3.2 `src/generated/CTFont.rs:977-984`), so a NULL aborts the process rather than returning "unknown". The port already handles the analogous nullable `copyDisplayName` in `name()` via a raw `CTFontCopyDisplayName` binding (`deferred_face.rs:246-262`), so the pattern is available.

_Evidence:_ `DeferredFace.zig:152-153` `const family_name = ct.font.copyAttribute(.family_name) orelse return "unknown";` vs `deferred_face.rs:286` `let family_name = unsafe { ct.font.family_name() };` with no NULL arm.

_Suggested fix:_ Declare a raw `CTFontCopyFamilyName(font: &CTFont) -> *mut CFString` and map NULL to `Ok(b"unknown")` copied into `buf`.

#### S86-F1 — `face.name()` error is swallowed in `collection()` instead of propagating

- **Status: CONFIRMED** · kind: missing-error-path · severity: medium
- Oracle: `src/font/SharedGridSet.zig:197-211, 228-242`
- rustty: `crates/rustty-font-shaper/src/shared_grid_set.rs:372-386, 406-420` (verified sites: `:374`, `:408`)

The reference builds the log argument with `try face.name(&name_buf)`. Zig evaluates the argument tuple eagerly, so any error from `DeferredFace.name` (CoreText `error.OutOfMemory` when the display name does not fit the 256-byte buffer; fontconfig `pattern.get(.fullname)` errors) aborts `collection()`, which aborts `ref()`, so no grid is created and the caller sees the error. rustty writes `let _ = face.name(&mut name_buf);` and continues to `add_deferred`, so the same condition silently succeeds and the face is added.

_Evidence:_ `SharedGridSet.zig:199-202` `log.info("font {s}: {s}", .{ field.name, try face.name(&name_buf) });` vs `shared_grid_set.rs:374-375` `// log.info(...)` + `let _ = face.name(&mut name_buf);` (and `:408-409` for the variations retry). `deferred_face.rs:669-673` even claims "the callers … discard it, as the reference does", which is not what `SharedGridSet.zig` does.

_Suggested fix:_ Use `let _name = face.name(&mut name_buf)?;` (mapping `FaceError` into `shared_grid_set::Error::Face`) at both sites.

#### S91-F2 — web_canvas `context()`: CSS font size formatted with integer truncation instead of `{d}` float formatting

- **Status: CONFIRMED** · kind: behavioral-difference · severity: medium
- Oracle: `src/font/face/web_canvas.zig:301-327`
- rustty: `crates/rustty-font-shaper/src/face/web_canvas.rs:596-604`

The reference builds the `ctx.font` value with `std.fmt.allocPrint(alloc, "{d}px {s}", .{ self.size.points, self.font_str })` where `points` is `f32`, so 13.5 points yields `"13.5px …"`. The port does `push_int(&mut font_val, self.size.points as i64)`, truncating fractional point sizes to an integer (`"13px …"`). For any non-integral `DesiredSize.points` the browser renders a different font size than the reference would, and all glyph measurements/renders scale accordingly (plus a guaranteed spurious font read-back mismatch).

_Evidence:_ `web_canvas.zig:310-314` vs `web_canvas.rs:597-600` with `push_int` (`web_canvas.rs:812-828` / verified `871-885`) handling integers only; `DesiredSize.points` is `f32` on both sides (`face/mod.rs:242`), and `rustty_wc_face_new` takes `pts: u16`, as the reference's export does (`web_canvas.zig:494`; it took `f32` until N73 corrected it, `.port/parity-closure/NEW-FINDINGS.md`).

_Suggested fix:_ Format `points` as Zig `{d}` would for `f32` (shortest round-trip decimal, no trailing zeros) so fractional sizes reach the canvas unchanged.

#### S91-F3 — web_canvas `initNamed` rejects inputs with a new `InvalidFontStr` error path the reference accepts

- **Status: DISPUTED** (refuted by the PRESENCE lens) · kind: behavioral-difference · severity: medium
- Oracle: `src/font/face/web_canvas.zig:48-77`
- rustty: `crates/rustty-font-shaper/src/face/web_canvas.rs:260-296, 748-772`

The reference `initNamed` accepts any `raw` byte string, copies it, creates the canvas and computes metrics; an invalid CSS value only produces a `log.warn` in `context()` when the read-back differs. The port adds `validate_css_font_str(raw)?`, returning `Error::InvalidFontStr` when `raw` starts with a font-file magic (0x00010000/true/ttcf/OTTO/wOFF/wOF2), is not valid UTF-8, or contains any control character. Such inputs now fail face construction (`face_new` returns null) where the reference would construct a face and proceed with the browser fallback font.

_Evidence:_ `web_canvas.zig:57-58` `const font_str = try alloc.dupe(u8, raw);` with no validation; `web_canvas.rs:271` `validate_css_font_str(raw)?;` and `:751-772` / `:816-838` returning `Err(Error::InvalidFontStr)`.

_Suggested fix:_ Either drop the up-front validation (keeping only the reference's post-set read-back warning) or document it as an intentional hardening divergence in the parity ledger.

_Refuting lens — PRESENCE (verbatim):_ "The finding's own suggested fix is 'document it as an intentional hardening divergence' — which is already done, explicitly, at web_canvas.rs:44-58, and the reference's post-set read-back warning is also kept (web_canvas.rs:605-611 -> font_did_not_set at :854-869), so nothing from the oracle path is dropped. The guard exists only because the port adds a modern-surface `Face::init(lib, source, opts)` shim (web_canvas.rs:316-318) that the reference Face does not have — the reference's initNamed is unreachable with non-CSS bytes by construction (its only caller is Wasm.face_new_, web_canvas.zig:497-503, which is fed a CSS string by the JS glue). For every input the reference can actually receive (a CSS text string), validate_css_font_str returns Ok and behaviour is identical. Not an observable parity gap on any reachable oracle path."

#### S94-F1 — CFReleaseThread replaced by an mpsc stand-in under a stale "deferred seam" PORT NOTE while rustty-os ships the real port

- **Status: DISPUTED** (refuted by the PRESENCE lens) · kind: other · severity: medium
- Oracle: `src/font/shaper/coretext.zig:76-81, 208-233, 255-262, 264-297`
- rustty: `crates/rustty-font-shaper/src/shaper/coretext.rs:36-55, 191-205, 421-449, 496-507, 511-563, 1381-1421`

The oracle's Shaper owns a heap `os.CFReleaseThread` (xev loop + bounded 64-slot BlockingQueue mailbox + wakeup/stop Async handles) spawned in `init` and stopped/joined in `deinit`; `endFrame` pushes the pool with `.forever` (blocking backpressure) and wakes the thread. rustty's `coretext.rs` carries a production PORT NOTE claiming `os.CFReleaseThread` is a DEFERRED seam (libxev all `unimplemented!()`) and substitutes a `std::thread` + unbounded `mpsc` channel. But `crates/rustty-os/src/cf_release_thread.rs` is a fully ported `Thread` (PORT STATUS: ported, real rustty-xev kqueue backend, `threadMain`/`drainMailbox`/`wakeupCallback`/`stopCallback`), and rustty-os sits below rustty-font-shaper in the dependency graph. Observable divergences of the stand-in: (a) unbounded channel vs bounded mailbox with producer backpressure; (b) stop semantics — the reference `stopCallback` stops the loop immediately and `deinit` destroys the mailbox, so pools still queued are never released, while the stand-in drains all queued Release messages before honoring Stop; (c) the reference's stop/wakeup notify error logs and the thread's abrupt-exit drain path do not exist.

_Evidence:_ `coretext.zig:76-81` field declarations; `:219-233` create/init/spawn; `:268-297` `mailbox.push(.., .{ .forever = {} })` then `wakeup.notify()`. `coretext.rs:43-44` says the thread "cannot spawn/run yet"; `:191-205`/`:1381-1421` wire `cf_release_tx: mpsc::Sender<Msg>` + `thread_main`. `rustty-os/src/cf_release_thread.rs:8` "PORT STATUS: ported", `:185-235` ported `threadMain_`/`drainMailbox`.

_Suggested fix:_ Add `rustty-os` as a macOS dependency of rustty-font-shaper and port init/deinit/endFrame against `rustty_os::cf_release_thread::Thread`, removing the stand-in module and the stale deferral notes.

_Refuting lens — PRESENCE (verbatim):_ "The observable behaviour the oracle's CFReleaseThread provides — CFRelease of the shaped-frame pool performed off the shaping thread, thread named \"cf_release\", stopped and joined in deinit — IS present: coretext.rs:434-438 spawns a named std::thread running cf_release::thread_main (coretext.rs:1410-1421, which CFReleases every ref of each Release pool and breaks on Stop); end_frame (coretext.rs:511-563) ships the pool to it and releases inline if the thread is gone; deinit (coretext.rs:496-507) sends Stop and joins. The three cited divergences are not user-observable functional differences: (a) unbounded mpsc vs the 64-slot BlockingQueue is a queue-capacity/backpressure resource characteristic with identical release semantics (perf-only); (b) the reference's stop-before-drain leaks queued pools at process teardown while the stand-in releases them — releasing more refs at shutdown is not an observable behaviour change for the terminal; (c) the log sites are the subject of S94-F4, not an independent defect. I also confirmed the real port exists and is complete at crates/rustty-os/src/cf_release_thread.rs:8 ('PORT STATUS: ported'), :180-235 (threadMain_/drainMailbox on the live rustty-xev kqueue backend), so the file-level functionality is present in the workspace; the coretext.rs:41-53 note claiming the xev seam is 'all unimplemented!()' is stale documentation only (rustty-global has no seam.rs any more), which is at most a low/doc-level issue, not the medium functional finding described."

#### S97-F1 — Sprite golden `U+2500...U+25FF-9x17+1.png` regenerated from rustty output; differs from oracle testdata by 1 pixel

- **Status: CONFIRMED** (both lenses reproduced the pixel diff independently) · kind: behavioral-difference · severity: medium
- Oracle: `src/font/sprite/Face.zig:563-591`, golden `src/font/sprite/testdata/U+2500...U+25FF-9x17+1.png`
- rustty: `crates/rustty-font/src/sprite/face.rs:990-1040` (test helper at `:886-960`), golden `crates/rustty-font/res/U+2500...U+25FF-9x17+1.png`

The oracle KAT `sprite face render all sprites` compares the rendered atlas against `src/font/sprite/testdata/*.png`; rustty compares against `crates/rustty-font/res/*.png`. 35 of the 36 files are pixel-identical to the oracle's, but `U+2500...U+25FF-9x17+1.png` was regenerated from rustty's own render and differs from the oracle golden at exactly one pixel: (x=133, y=381) oracle coverage 79, rustty 95 (delta 16/255), inside glyph cell (10,15) of the batch — i.e. U+25FA (`geometric_shapes` `draw25F8_25FA` outline triangle) at cell 9×17 thickness 1. The sprite face therefore renders that glyph differently from the oracle at those metrics, and the ported test cannot detect it because its reference was replaced.

_Evidence:_ Both PNG sets decoded with a stdlib-only Python decoder: all grayscale, same dimensions; only this file differs, pixdiff=1 at (133,381), 79 vs 95. `face.rs:897-906` builds `ref_path` from `CARGO_MANIFEST_DIR/res/`. `.port/phase-3/NON_PORTS.md:239-252` states the file was "regenerated from rustty's own render rather than copied from the oracle". Root cause is attributed there to rustty-z2d's stroke/offset pipeline (a different shard), but the divergence surfaces in the sprite face.

_Suggested fix:_ Fix the 1-pixel divergence in rustty-z2d (inner stroke of the U+25FA outline at line_width 1) and restore the oracle's byte-identical golden; until then, mark the KAT as knowingly weakened for this file.

#### S100-F1 — Action numeric parameter parsing rejects inputs Zig std.fmt accepts (`_` separators, `-0` for unsigned, hex floats)

- **Status: CONFIRMED** · kind: behavioral-difference · severity: medium
- Oracle: `src/input/Binding.zig:1224-1230`
- rustty: `crates/rustty-input/src/binding.rs:1952-1990`

The oracle's `parseInt` is `std.fmt.parseInt(T, value, 10)` and `parseFloat` is `std.fmt.parseFloat(T, value)`. Zig's parseInt explicitly ignores `_` between digits, parseFloat accepts `_` separators and hex floats like `0x1p1`, and `parseIntWithSign` accepts `-0` for unsigned result types. The rustty helpers `parse_int_i16/isize/usize/u16` and `parse_float_f32` go through `str::parse`, which rejects all of these, returning `Error::InvalidFormat`. The port's own PORT NOTE (`binding.rs:1952-1958`) acknowledges the `_` gap. Affected actions: `increase_font_size`, `decrease_font_size`, `set_font_size`, `scroll_page_fractional` (f32); `scroll_to_row`, `goto_tab` (usize); `scroll_page_lines`, `jump_to_prompt` (i16); `move_tab` (isize); `resize_split` second tuple field (u16).

_Evidence:_ `Binding.zig:1227`/`:1231`; `binding.rs:1959-1963` `s.parse::<i16>().ok()`, `:1985-1990` `s.parse::<f32>()`. Examples: `keybind = ctrl+a=goto_tab:1_0` → Zig `goto_tab=10`, rustty InvalidFormat; `goto_tab:-0` → Zig 0, rustty InvalidFormat; `set_font_size:0x1.8p3` → Zig 12.0, rustty InvalidFormat.

_Suggested fix:_ Port Zig's `parseIntWithSign` semantics and a `parseFloat` wrapper that strips valid underscores and handles the `0x` hex-float prefix before delegating to `str::parse` (or port `std.fmt.parseFloat` into `rustty-zigstd`).

### Low severity

#### S84-F1 — All log.warn/info/debug statements in CodepointResolver and Collection are dropped (kept only as comments)

- **Status: CONFIRMED** · kind: other · severity: low
- Oracle: `src/font/CodepointResolver.zig:138-140, 176, 195-199, 218, 265-268, 290-293` and `Collection.zig:342-345, 375, 384, 392, 405, 413, 421, 428, 438, 450, 458, 483-485, 506-508`
- rustty: `codepoint_resolver.rs:193,248,294,329,366,447,495,564`; `collection.rs:479,511,521,535,547,557,570,582,594,612,624`

The oracle emits scoped log lines ("codepoint override failed …", "fallback search failed with error …", "found codepoint 0x{X} in fallback face={s}", "error loading regular entry={d} err={}", "italic style not available and synthetic italic disabled", "synthetic bold face created", "bold italic style not available, using italic font", …). rustty carries every one as a `// log.xxx(...)` comment and emits nothing; the crate has no logging facility. Control flow around each log — including the `deferred_face.name(&buf)` / `face.name(&buf)` calls that only fed the log — is preserved, so this is diagnostics-only. Reported once for the shard as a crate-wide convention.

_Evidence:_ `codepoint_resolver.rs:180-182` `Err(_err) => { // log.warn("codepoint override failed …") }` vs `CodepointResolver.zig:138-140`; `grep -rl 'warn!(|info!(|debug!(' crates/rustty-font-shaper/src` returns nothing, and the crate's Cargo.toml declares no log/tracing dependency.

_Suggested fix:_ Route the commented log lines through whatever logging facility rustty adopts (scoped `.font_codepoint_resolver` / `.font_collection`), or record the crate-wide log elision as an accepted deviation.

#### S85-F3 — `load_coretext` drops the `errdefer face.deinit()` — HarfBuzz font handle leaks when `set_variations` fails

- **Status: CONFIRMED** · kind: missing-error-path · severity: low
- Oracle: `src/font/DeferredFace.zig:248-258`
- rustty: `crates/rustty-font-shaper/src/deferred_face.rs:355-362`

Oracle `loadCoreText` has `errdefer face.deinit()` so a failing `setVariations` tears down the freshly created face. The port writes `face.set_variations(&ct.variations, opts)?;` and lets `face` drop. The CoreText `Face` has no `Drop` impl and `HARFBUZZ_SHAPER` is `true`, so `deinit` (`face/coretext.rs:333-349`) is what destroys `hb_font`; skipping it leaks the HarfBuzz font on this path. The fontconfig (`deferred_face.rs:725-737`) and Windows (`:951-957`) arms of the same port DO call `face.deinit()` on error.

_Evidence:_ `DeferredFace.zig:255-256` vs `deferred_face.rs:358-361` (`// errdefer face.deinit();` comment only).

_Suggested fix:_ Match the fontconfig/Windows arms: `match face.set_variations(..) { Ok(()) => Ok(face), Err(e) => { face.deinit(); Err(e) } }`.

#### S85-F4 — formatConfig percent/absolute tests assert only the value, not the full formatted entry line

- **Status: CONFIRMED** · kind: weakened-assertion · severity: low
- Oracle: `src/font/Metrics.zig:561-585`
- rustty: `crates/rustty-font/src/metrics.rs:944-978`

The oracle tests run `p.formatEntry(configpkg.entryFormatter("a", &buf.writer))` and assert `"a = 24%\n"` / `"a = -30\n"`. The port asserts `b"24%"` / `b"-30"`; the `key = …\n` wrapping (now implemented in `config_struct.rs:8610-8626` via `rustty_cfg_fmt::format_entry_string`) is not asserted anywhere. The PORT NOTE at `metrics.rs:945-956` still says the config crate "is not yet carved", which is stale.

_Evidence:_ `Metrics.zig:571-572` vs `metrics.rs:964-966`.

_Suggested fix:_ Add a rustty-config (or rustty-cfg-fmt) test that formats the modifier under key "a" through the real entry formatter and asserts `b"a = 24%\n"` / `b"a = -30\n"`; update the stale PORT NOTE.

#### S85-F5 — Fontconfig `name`/`familyName`/`load` map `pattern.get` failures to `FaceError::OutOfMemory` instead of propagating the fontconfig error

- **Status: DISPUTED** (refuted by the SEMANTIC lens) · kind: behavioral-difference · severity: low
- Oracle: `src/font/DeferredFace.zig:153, 183, 226-229`
- rustty: `crates/rustty-font-shaper/src/deferred_face.rs:643-647, 680-684, 713-722`

The oracle's `try fc.pattern.get(.family/.fullname/.file/.index, 0)` propagates the fontconfig error atom (NoMatch/TypeMismatch) out of `familyName`/`name`/`load`. The port (documented in its ERROR-ATOM NOTE at `:660-673`) collapses any `get` failure or wrong-typed value into `FaceError::OutOfMemory`. The returned error value differs; all current callers discard it, and render-prepared patterns always carry FC_FAMILY/FC_FILE/FC_INDEX, so the difference is only observable for a pattern missing FC_FULLNAME or a hypothetical caller inspecting the error.

_Evidence:_ `DeferredFace.zig:153` `return (try fc.pattern.get(.family, 0)).string` vs `deferred_face.rs:643-647` `match pattern.get(fc::Property::Family, 0) { Ok(fc::Value::String(s)) => s, _ => return Err(oom()) }`.

_Suggested fix:_ Add a `Fontconfig(fc::Error)` variant to the freetype `FaceError` (or a wrapping DeferredFace error) so the fontconfig atom survives as in the oracle.

_Refuting lens — SEMANTIC (verbatim):_ "Read DeferredFace.zig:150-153/179-181/226-229 and deferred_face.rs:637-737. The control flow is identical (success value vs a failure return); only the error atom's identity differs, and no caller inspects it: CodepointResolver.zig:205 does `deferred_face.name(&buf) catch \"<error>\"`, Collection.zig:487/512 use `else |_| {}`, and the rustty callers likewise discard. There is no input for which the two trees produce a different observable result — the error value is never rendered, matched on, or propagated to the user. Additionally the oracle's own wrong-type path (`(try get(...)).string` on a non-string union) is a Zig safety panic/UB, not a distinct error atom, so 'propagating the fontconfig error' is only accurate for the get-failure case. Documented error-set unification, not an observable divergence."

#### S85-F6 — CoreText `name`/`familyName` always copy into `buf`, dropping the unlimited `cstringPtr` zero-copy path

- **Status: CONFIRMED** · kind: behavioral-difference · severity: low
- Oracle: `src/font/DeferredFace.zig:154-157, 180-190`
- rustty: `crates/rustty-font-shaper/src/deferred_face.rs:286-296, 317-327`

The oracle first tries `cstringPtr(.utf8)` (a pointer into CFString-owned storage, no buffer limit) and only on failure falls back to `cstring(buf, .utf8)`, which errors OutOfMemory if the name does not fit. The port always converts via `to_string()` and copies into `buf`, returning OutOfMemory whenever `bytes.len() > buf.len()`. For a name longer than the caller's buffer (1024 bytes at every current call site) the oracle can still succeed through the pointer path while the port errors. Practically unreachable with real fonts.

_Evidence:_ `DeferredFace.zig:154-157` vs `deferred_face.rs:288-292` `if bytes.len() > buf.len() { return Err(oom()); }`.

_Suggested fix:_ Accept as a documented lifetime-unification choice, or expose an owned-`Vec<u8>` name accessor so long names cannot fail.

#### S85-F7 — Stale comment: `cursor_thickness` is at field index 10, not 11

- **Status: DISPUTED** (refuted by the SEMANTIC lens) · kind: other · severity: low
- Oracle: `src/font/Metrics.zig:590-608`
- rustty: `crates/rustty-font/src/metrics.rs:887`

The PORT NOTE above `Key` says "`cursor_thickness` is at index 11" but the Metrics field order (and the enum discriminants right below, `CursorThickness = 10`, `CursorHeight = 11`) place it at index 10. The discriminants themselves are correct; only the comment is wrong.

_Evidence:_ `metrics.rs:887` vs `metrics.rs:899-900` and the oracle field order (`cursor_thickness` is the 11th field, 0-based index 10).

_Suggested fix:_ Change the comment to index 10.

_Refuting lens — SEMANTIC (verbatim):_ "Verified both sides. Oracle Metrics.zig:7-54 field order puts cursor_thickness as the 11th field (0-based index 10), and the oracle's comptime Key (Metrics.zig:589-607) assigns tag = field index, so CursorThickness = 10. rustty crates/rustty-font/src/metrics.rs:889-906 declares CursorThickness = 10, CursorHeight = 11 — i.e. the actual enum discriminants match the oracle exactly for all 17 keys. The only defect is the PORT NOTE text at metrics.rs:887 ('`cursor_thickness` is at index 11'), a rustty-internal comment with no oracle counterpart and no observable behaviour: no input or state yields a different value, error, or ordering. Under the semantic-equivalence lens this is not a functional-parity divergence (comment-only, arguably just 0-based vs ordinal phrasing). Refuted; at most an editorial nit."

#### S86-F2 — All log output of SharedGridSet is dropped (no logging in the shaper crate)

- **Status: CONFIRMED** · kind: other · severity: low
- Oracle: `src/font/SharedGridSet.zig:109, 120, 182-185, 199-202, 230-233, 246-249`
- rustty: `crates/rustty-font-shaper/src/shared_grid_set.rs:189, 202, 355, 374, 408, 425`

The reference emits `log.debug("found cached grid for font config")`, `log.debug("initializing new grid for font config")`, `log.warn("font discovery not available, cannot search for fonts")`, `log.info("font {s}: {s}")` per discovered primary face, and `log.warn("font-family {s} not found: {s}")`. rustty keeps all of these only as comments; the crate has no log/tracing dependency. The "font-family … not found" warning in particular is the only user-visible signal that a configured font-family was ignored.

_Evidence:_ `shared_grid_set.rs:189`, `:355`, `:425` are comment-only; `grep -rln 'log::' crates/rustty-font-shaper/src` returns only the unrelated wasm shim.

_Suggested fix:_ Add a `log` dependency (scoped target `font_shared_grid_set`) and emit the five messages with the same levels and text.

#### S86-F3 — `getIndex` test no longer proves the second pass is served from the cache only

- **Status: CONFIRMED** · kind: weakened-assertion · severity: low
- Oracle: `src/font/SharedGrid.zig:565-574`
- rustty: `crates/rustty-font-shaper/src/shared_grid.rs:815-832`

The reference sets `grid.resolver = undefined` (restored via defer) before the second ASCII pass so that any resolver hit would read garbage/crash, proving the codepoint cache is authoritative. rustty re-runs the same loop against the live resolver, so a regression where `get_index` bypasses the cache and re-resolves would still pass. The `expectEqual` assertions themselves are present with equal count.

_Evidence:_ `SharedGrid.zig:566-568` vs the PORT NOTE at `shared_grid.rs:816-826` ("Rust has no undefined … the cache-only guarantee is instead asserted by re-querying the same range").

_Suggested fix:_ Swap in a poisoned resolver (e.g. an empty `Collection`) so any resolver hit returns None/fails, then restore it after the loop.

#### S86-F4 — Tripwire-based tests are `#[ignore]`d in release-profile test runs

- **Status: CONFIRMED** · kind: missing-test · severity: low
- Oracle: `src/font/SharedGrid.zig:576-656`
- rustty: `crates/rustty-font-shaper/src/shared_grid.rs:838-846, 909-917` (attributes at `:845`, `:916`)

The reference's tripwire is enabled whenever `builtin.is_test`, so "renderGlyph error after cache insert rolls back cache entry" and "init error" run under every test build. rustty's tripwire is `cfg!(any(test, debug_assertions, feature = "tripwire-test"))` evaluated in `rustty-util`, and `rustty-font-shaper` has no dev-dependency enabling `tripwire-test`, so both tests carry `#[cfg_attr(not(debug_assertions), ignore)]` and are skipped in `cargo test --release`. Assertions are complete when they do run.

_Evidence:_ `rustty-util/src/tripwire.rs:198-200`; `crates/rustty-vt/Cargo.toml:240-248` shows the in-repo pattern (`features = ["tripwire-test"]`) that rustty-font-shaper lacks.

_Suggested fix:_ Gate the tripwire on `cfg!(test)` (or enable the `tripwire-test` feature for this crate's tests) so the tests run in all test profiles.

#### S87-F1 — CoreText allocation failures panic instead of returning an error / zero score

- **Status: DISPUTED** (refuted by the SEMANTIC lens) · kind: missing-error-path · severity: low (corrected from medium)
- Oracle: `src/font/discovery.zig:681-690, 967-973, 1027-1035, 244`
- rustty: `crates/rustty-font-shaper/src/discovery.rs:1649-1654` (score), `2062-2066` (DiscoverIterator::next), `1137-1138` (to_core_text_descriptor)

The oracle handles NULL results from `CTFontCreateWithFontDescriptor`, `CTFontDescriptorCreateCopyWithAttributes` and `CTFontDescriptorCreateWithAttributes` explicitly: `Score.score` does `createWithFontDescriptor(ct_desc, 12) catch return self` (an unloadable font scores 0 and sorts last), and `DiscoverIterator.next` / `toCoreTextDescriptor` propagate `error.OutOfMemory` via `try`. The port calls objc2's non-null wrappers, which unwrap a NULL into a panic (the port's own PORT NOTE acknowledges the `catch return self` path is dropped). The port already declares a raw `sys` extern block for `CTFontCreateForString` / `CTFontCreateWithName`, so the fix pattern exists in-file.

_Evidence:_ `discovery.zig:681-690` vs `discovery.rs:1591`/`:1649-1654`; objc2-core-text-0.3.2 `CTFont.rs:285-288` `.expect("function was marked as returning non-null, but actually returned NULL")`.

_Suggested fix:_ Declare the three raw CoreText symbols in the existing `sys` extern block and map NULL to `return this` in `Score::score` and `Err(Error::OutOfMemory)` in the other two.

_Refuting lens — SEMANTIC (verbatim):_ "Read discovery.zig:681-690 (`createWithFontDescriptor(...) catch return self`), :1021-1029, :244 and the port at discovery.rs:1642-1654, 2057/2065, 1156. The port's calls go through objc2-core-text 0.3.2 wrappers (CTFont.rs:273-289, CTFontDescriptor.rs:496-506 and :536-550) whose extern decls return `Option<NonNull<..>>` and `.expect()` a NULL. Those three CoreText symbols are annotated `nonnull` in Apple's headers (that's why objc2 generates non-optional returns), and the oracle's own comment at discovery.rs:1651-1653 / the ghostty comment at discovery.zig:427 (\"CTFontCreateWithName never returns null\") reflects the same contract; the Zig wrapper's NULL->error.OutOfMemory arm is purely defensive. The divergence is therefore confined to code unreachable for any input, which the ground rules exclude. No reachable input produces a different score, ordering or error."

#### S87-F2 — `discoverExactFamily`: family names ≥ 256 bytes are rejected by the oracle but accepted by the port

- **Status: CONFIRMED** · kind: behavioral-difference · severity: low
- Oracle: `src/font/discovery.zig:429-441`
- rustty: `crates/rustty-font-shaper/src/discovery.rs:1278-1290`

The oracle copies the CTFont family name into a 256-byte stack buffer via `cstring(&buf, .utf8)`; if the name plus NUL does not fit, `orelse break :found false` makes the function return null. The port converts with `to_string()` and compares bytes directly, so a ≥256-byte family name that equals the request returns `Some(face)` where the oracle returns null. The PORT NOTE explicitly collapses this arm.

_Evidence:_ `discovery.zig:432-437` vs `discovery.rs:1285-1286` `let actual_s = actual.to_string(); actual_s.as_bytes() == family`.

_Suggested fix:_ Add `if actual_s.len() >= 256 { false } else { actual_s.as_bytes() == family }`, mirroring the buffer-size arms the port already reproduces elsewhere (64-byte LastResort, 128/1024-byte Score buffers).

#### S87-F3 — `embedded.bold` / `italic` / `bold_italic` left as a PORT NOTE deferral

- **Status: CONFIRMED** · kind: stub-or-unimplemented · severity: low
- Oracle: `src/font/embedded.zig:15-18`
- rustty: `crates/rustty-font/src/embedded.rs:53-69`

The oracle exposes four static JetBrains Mono faces (`regular`, `bold`, `italic`, `bold_italic`). The port embeds `regular` and leaves the other three as a "PORT NOTE (DEFERRED — build-anonymous imports)" comment with no constant. The oracle marks them "currently unused" and no ghostty `src/` use-site exists, so this is not user-visible, but the declarations are missing.

_Evidence:_ `embedded.zig:16-18` `@embedFile(...)` vs `embedded.rs:66-69` comment-only.

_Suggested fix:_ Vendor JetBrainsMono-Bold/-Italic/-BoldItalic.ttf into `crates/rustty-font/res` and add the three `include_bytes!` constants.

#### S87-F4 — GTK boxed GType for DesiredSize registered as `GhosttyFontSizeRequest` instead of `GhosttyFontDesiredSize`

- **Status: CONFIRMED** · kind: different-constant · severity: low
- Oracle: `src/font/face.zig:63-72`
- rustty: `crates/rustty-apprt/src/gtk/class/surface.rs:164-166` (vs `crates/rustty-font-shaper/src/face/mod.rs:257-271`)

On the GTK runtime the oracle registers `DesiredSize` itself as a boxed GObject type named `GhosttyFontDesiredSize`, which is what the `font-size-request` property carries. The port sets `DesiredSize::GET_G_OBJECT_TYPE: () = ()` (the `.none` arm only) and relocates the boxed registration to rustty-apprt as a `FontSizeRequest(DesiredSize)` newtype registered under `GhosttyFontSizeRequest`. The property plumbing works, but the registered type name (observable via `g_type_name` / GObject introspection) differs.

_Evidence:_ `face.zig:64-68` `defineBoxed(DesiredSize, .{ .name = "GhosttyFontDesiredSize" })` vs `surface.rs:164-166` `#[boxed_type(name = "GhosttyFontSizeRequest", nullable)]`.

_Suggested fix:_ Rename the `boxed_type` name to `"GhosttyFontDesiredSize"` (kept as the ghostty name per the `ghostty_*` alias policy).

#### S88-F1 — `diffAtlas` failure-path diagnostic files are not written

- **Status: DISPUTED** (refuted by the SEMANTIC lens) · kind: other · severity: low
- Oracle: `src/font/glyf_rasterize_png_test.zig:116-142, 156-196`
- rustty: `crates/rustty-font/src/glyf_rasterize/glyf_rasterize_png_test.rs:149-163, 404-405, 432-452`

On a missing reference or any mismatch, the oracle copies the generated PNG into the cwd as `glyf_rasterize_test.png` and, on a pixel mismatch, writes a red/green RGB diff image `./glyf_rasterize_diff.png` and logs the three paths. The rustty test panics with a per-pixel message and writes no files. Failure/success outcome is identical in every arm; only the on-disk debugging artifacts differ. Documented as a deliberate drop in the PORT NOTE.

_Evidence:_ Oracle `129-131` `copyFileAbsolute(generated_path, test_path)`, `156-190` diff surface + `writeToPNGFile`; rustty `404-405` `fs::read(REF_PATH).unwrap_or_else(...)`, `432-452` per-pixel `assert_eq!` only.

_Suggested fix:_ Optionally write the generated PNG and a red/green diff PNG on failure via the `png` encoder, logging the three paths as the oracle does.

_Refuting lens — SEMANTIC (verbatim):_ "Read oracle src/font/glyf_rasterize_png_test.zig:150-196 and rustty crates/rustty-font/src/glyf_rasterize/glyf_rasterize_png_test.rs:228-259, 404-452. Every decision arm is preserved with identical pass/fail outcome: byte-equal -> pass (rs:230-232 == zig:137); missing reference -> fail (rs:404-405 panic == zig:126-133 return true); dimension mismatch -> fail (rs:236-245 assert_eq == zig:147-154); bytes-differ-pixels-match -> fail (rs:437-441 panic == zig:180-187); pixel diff -> fail (rs:432-452 per-pixel assert_eq == zig:189-190 return true, asserted by expect(!...) at zig:302). The only delta is on-disk debugging artifacts (glyf_rasterize_test.png copy, red/green glyf_rasterize_diff.png) written on the failing paths of a test that fails either way, plus log text. No input/state yields a different test result; this is diagnostic output, not observable behaviour, and the drop is documented in the PORT NOTE at rs:149-163."

#### S88-F2 — Added `RUSTTY_REGEN_FONT_KAT_PNGS` arm makes the KAT pass (and rewrite the reference) where the oracle fails

- **Status: DISPUTED** (refuted by the SEMANTIC lens) · kind: other · severity: low
- Oracle: `src/font/glyf_rasterize_png_test.zig:138, 180-187, 302`
- rustty: `crates/rustty-font/src/glyf_rasterize/glyf_rasterize_png_test.rs:409-420`

When `RUSTTY_REGEN_FONT_KAT_PNGS` is set and the encoded bytes differ but decoded pixels match, the rustty test overwrites `res/glyf_rasterize.png` and returns success. The oracle has no such arm: bytes-differ-but-pixels-match is always a failure. With the env var unset the behaviour is identical; the arm is gated so it cannot mask a pixel change.

_Evidence:_ rustty `414-420` `if env::var_os("RUSTTY_REGEN_FONT_KAT_PNGS").is_some() && outcome == AtlasDiff::BytesDifferPixelsMatch { fs::write(REF_PATH, &generated_bytes).unwrap(); … return; }` vs oracle `180-187` unconditional failure.

_Suggested fix:_ Acceptable as a dev affordance; for strict test-behaviour parity, move regeneration into an xtask/ignored test rather than an env-gated pass arm inside the KAT.

_Refuting lens — SEMANTIC (verbatim):_ "Read rustty glyf_rasterize_png_test.rs:409-431: the regeneration arm is gated on env::var_os(\"RUSTTY_REGEN_FONT_KAT_PNGS\").is_some() AND outcome == AtlasDiff::BytesDifferPixelsMatch, so with the var unset (the state of every CI/default run) the flow is byte-for-byte the oracle's: PixelsDiffer still falls through to the failure arm at rs:432-452 and BytesDifferPixelsMatch still panics at rs:437-441, matching zig:180-187 + zig:302. The added arm cannot loosen any assertion on the default path and, being gated on decoded-pixel equality, cannot mask a visual change; the AtlasDiff classifier itself is separately pinned by the proving test at rs:462-503. An opt-in developer affordance with no default-path behavioural divergence is not an observable functional difference."

#### S88-F3 — Stale crate doc claims `face/web_canvas.zig` is unported although the ports exist

- **Status: DISPUTED** (refuted by the SEMANTIC lens) · kind: other · severity: low
- Oracle: `src/font/main.zig:34-41`
- rustty: `crates/rustty-font-shaper/src/lib.rs:11-21` (vs `src/face/web_canvas.rs:1-6`)

The rustty-font-shaper module doc lists `face/web_canvas.zig` as an "honest residual … unported", but `face/web_canvas.rs` (header: "PORTS: ghostty src/font/face/web_canvas.zig (561 loc)") and `shaper/web_canvas.rs` exist and are wired for the `main.zig` comptime wasm retention through the `ghostty-wasm-abi` feature. Doc-only drift; no functional impact.

_Evidence:_ `lib.rs:19-21` vs `face/web_canvas.rs:3`; `rustty-wasm/src/lib.rs:49-53` describes retention of all four `font/main.zig` wasm groups.

_Suggested fix:_ Update the module doc to drop the "web_canvas unported" residual.

_Refuting lens — SEMANTIC (verbatim):_ "The claimed drift is real but is not a parity divergence. crates/rustty-font-shaper/src/lib.rs:19-21 does say 'face/web_canvas.zig (wasm browser face, unported)', while crates/rustty-font-shaper/src/face/web_canvas.rs exists with header 'PORTS: ghostty src/font/face/web_canvas.zig (561 loc)' (lines 1-5) and shaper/web_canvas.rs covers the oracle's comptime wasm retention at src/font/main.zig:33-41. So the oracle behaviour IS ported; the only defect is a stale sentence in a rustty-internal module doc that has no oracle counterpart text. Nothing in the ported code, tests, or observable behaviour differs from the oracle, so under the semantic-equivalence lens this is refuted as a parity finding (worth a doc cleanup, not a shard finding)."

#### S90-F2 — All face/coretext log output (Debug variation-axes dump, atlas-depth warn, table-parse warns, ic_width discard warn) is comment-only

- **Status: CONFIRMED** · kind: other · severity: low
- Oracle: `src/font/face/coretext.zig:119-162, 435-441, 830-841, 632-635/645-648/658-661/671-674`
- rustty: `crates/rustty-font-shaper/src/face/coretext.rs:281-320, 664-676, 1128-1158, 833-866`

The oracle emits `log.debug` for each variation axis (name/id/min/max/def) in Debug builds, `log.warn` when the atlas depth mismatches the font colour depth, `log.warn("error parsing {head,post,OS/2,hhea} table: {}")` on parse failure, and `log.warn` (including the font name) when the 水 glyph width exceeds its advance. rustty has no logging facade in this crate and drops all of them. Control flow and return values are identical. The Debug-only axes dump also exercised `copyAttribute(.variation_axes)` / `FontVariationAxisKey` reads that are now absent.

_Evidence:_ `coretext.zig:119-162`, `:435-441`, `:830-841` vs the verbatim comment blocks at `coretext.rs:281-320`, `:664-668` (then `return Err(Error::InvalidAtlasFormat)`), `:1152-1158` (then `break 'ic_width None`).

_Suggested fix:_ Wire the crate's logging facade and emit the same messages, keeping the Debug-only gating via `cfg(debug_assertions)`.

#### S90-F3 — `Face::name` errors on family names longer than `buf` even when CoreFoundation could return them without copying

- **Status: CONFIRMED** · kind: behavioral-difference · severity: low
- Oracle: `src/font/face/coretext.zig:210-219`
- rustty: `crates/rustty-font-shaper/src/face/coretext.rs:383-395`

The oracle first tries `family_name.cstringPtr(.utf8)` and returns that CF-owned pointer regardless of `buf` length; only when CF cannot expose the storage does it copy into `buf` and fail with `error.OutOfMemory` if it does not fit. rustty always decodes to a String and copies, returning `Error::OutOfMemory` whenever `bytes.len() > buf.len()`. All in-tree callers pass a 1024-byte buffer, so this is only reachable with pathological font names.

_Evidence:_ `coretext.zig:211-212` vs `coretext.rs:384-393`.

_Suggested fix:_ Try `CFStringGetCStringPtr` first and return it; fall back to the buffer copy only when the pointer path returns null.

#### S90-F4 — u32→CGGlyph conversion silently truncates (`as CGGlyph`) where the oracle uses checked `@intCast`

- **Status: DISPUTED** (refuted by the SEMANTIC lens) · kind: behavioral-difference · severity: low
- Oracle: `src/font/face/coretext.zig:297, 808-819`
- rustty: `crates/rustty-font-shaper/src/face/coretext.rs:517, 1106` (reported as 496 / 1122)

`renderGlyph` and the ic_width block convert the u32 glyph index to a 16-bit `CGGlyph` with `@intCast`, which traps in safe builds if the value exceeds u16. rustty uses `as CGGlyph`, which wraps silently, so an out-of-range glyph id renders glyph `id & 0xFFFF` instead of panicking. Only reachable with invalid input.

_Evidence:_ `coretext.zig:297` `var glyphs = [_]macos.graphics.Glyph{@intCast(glyph_index)};` vs `coretext.rs:517` `[glyph_index as CGGlyph]`.

_Suggested fix:_ Use `u16::try_from(glyph_index).expect(...)` / `debug_assert!` to keep the safe-build trap semantics.

_Refuting lens — SEMANTIC (verbatim):_ "Verified oracle coretext.zig:297 `@intCast(glyph_index)` and :811/:817 `&.{@intCast(glyph)}` vs rustty coretext.rs:517 `[glyph_index as CGGlyph]` and :1106 `[glyph as CGGlyph]`. The difference only materializes for a glyph id >= 2^16, and every producer of these ids is glyph_index() (coretext.rs:1090-1122 / oracle glyphIndex), which returns a CGGlyph-derived u16; sprite/graphics codepoints are routed to a different face type before reaching here. So the divergence is confined to an unreachable/invalid-input path, and in Ghostty's shipping ReleaseFast build @intCast is not a checked trap either, making the practical behaviour the same truncation. Not an observable functional difference."

#### S90-F5 — test "emoji" setup deviates: base CTFont created at 18pt instead of 12pt

- **Status: CONFIRMED** · kind: missing-test · severity: low
- Oracle: `src/font/face/coretext.zig:1010-1027`
- rustty: `crates/rustty-font-shaper/src/face/coretext.rs:1419-1436` (helper `named_font` at `:1361-1365`)

The oracle creates the descriptor and base CTFont at size 12 and then `initFontCopy`s at 18 points, exercising the size-changing copy path; rustty's `named_font("Apple Color Emoji", 18.0)` builds the base at 18 and copies at 18, so the copy is a no-op resize. The assertion is identical, so this is a setup-only deviation. (The sibling `name` test correctly keeps 12.0, so it is a one-off.)

_Evidence:_ `coretext.zig:1014-1021` vs `coretext.rs:1420-1423`.

_Suggested fix:_ Call `named_font("Apple Color Emoji", 12.0)` and keep `opts(18.0)` for the copy.

#### S91-F5 — web_canvas `context()`: font read-back check truncates at 256 bytes

- **Status: CONFIRMED** · kind: behavioral-difference · severity: low
- Oracle: `src/font/face/web_canvas.zig:316-324`
- rustty: `crates/rustty-font-shaper/src/face/web_canvas.rs:605-611`

The reference reads back the full `ctx.font` string (`ctx.getAlloc(js.String, alloc, "font")`) and compares it to `font_val`. The port reads into a fixed 256-byte stack buffer; any font string longer than 256 bytes is truncated and always compares unequal, producing a spurious "canvas font didn't set" warning (log-only effect).

_Evidence:_ `web_canvas.zig:316` vs `web_canvas.rs:606-607` `let mut check = [0u8; 256];`.

_Suggested fix:_ Size the read-back buffer from the returned length, or use a `Vec` sized `font_val.len()+1` so the comparison is exact.

#### S91-F6 — `face_new`: presentation u16 mapped nonzero⇒emoji instead of `@enumFromInt` on a u1 enum

- **Status: DISPUTED** (refuted by the PRESENCE lens) · kind: behavioral-difference · severity: low
- Oracle: `src/font/face/web_canvas.zig:494-511`
- rustty: `crates/rustty-font-shaper/src/face/web_canvas.rs:961-980` (and `rustty_wc_face_new` at `:906-910`)

The reference does `@enumFromInt(presentation)` into `Presentation = enum(u1) { text=0, emoji=1 }`; any value other than 0/1 is a safety-checked panic (or UB in ReleaseFast). The port maps 0 ⇒ Text and every other value ⇒ Emoji, silently accepting out-of-range values.

_Evidence:_ `web_canvas.zig:498-503` vs `web_canvas.rs:965-969` `let pres = if p == 0 { Presentation::Text } else { Presentation::Emoji };`.

_Suggested fix:_ Return null (the `catch null` path) for presentation > 1 to mirror the reference's rejection of invalid values.

_Refuting lens — PRESENCE (verbatim):_ "The reference behaviour for presentation > 1 is not defined behaviour to match: `@enumFromInt(presentation)` into `Presentation = enum(u1)` (src/font/main.zig:62-65) from web_canvas.zig:498-503 is a safety-check panic in Debug/ReleaseSafe and illegal behaviour in the ReleaseFast/ReleaseSmall builds the wasm artifact uses — there is no observable value the port could reproduce. The only caller is the JS glue, which passes 0/1, and for those inputs the port's mapping (web_canvas.rs:906-910, :1011-1015) is identical to the reference. The mapping choice is written out in the PORT NOTE at web_canvas.rs:1003-1006. Not a defensible parity gap; suggesting `return null` would itself invent behaviour the reference does not have."

#### S91-F7 — web_canvas `log.debug`/`log.warn` calls dropped (face initialized, metrics, error rendering glyph, error adding debug canvas)

- **Status: CONFIRMED** · kind: behavioral-difference · severity: low
- Oracle: `src/font/face/web_canvas.zig:76, 296, 518-524, 528-531`
- rustty: `crates/rustty-font-shaper/src/face/web_canvas.rs:298, 557, 1050, 1104` (reported as 297/565/1005-1008/1061-1063)

The reference emits `log.debug("face initialized: {s}")`, `log.debug("metrics font={s} value={}")`, `log.warn("error rendering glyph err={}")` and `log.warn("error adding debug canvas err={}")`. The port carries these as comments only. (The "canvas font didn't set" warning IS preserved via `font_did_not_set` under `ghostty-wasm-abi`.) Log-only difference.

_Evidence:_ `web_canvas.zig:76`, `:521` vs `web_canvas.rs:298` `// log.debug("face initialized: {s}", .{raw});` and the comment-only Err arms.

_Suggested fix:_ Route through `rustty_os::wasm::log::freestanding::log` as `font_did_not_set` already does.

#### S91-F8 — freetype `initFace`: Debug-mode variation-axes diagnostic block omitted

- **Status: CONFIRMED** · kind: missing-branch · severity: low
- Oracle: `src/font/face/freetype.zig:116-137`
- rustty: `crates/rustty-font-shaper/src/face/freetype.rs:266-290`

In Debug builds the reference logs the font name and every MM axis (name/id/min/max/def). The block is ported as a comment only. It is diagnostic, but note that in Debug builds the reference's `try result.name(&buf)` / `try face.getMMVar()` inside the block can also turn a name/MMVar failure into an `initFace` error; release behaviour is identical.

_Evidence:_ `freetype.zig:118-135` `if (comptime builtin.mode == .Debug) mm: { … log.debug("variation axis: …") }` vs the commented-out block in the port.

_Suggested fix:_ None required for release parity; optionally re-enable under `#[cfg(debug_assertions)]` with a logging facade.

#### S91-F9 — freetype `name()`: family string copied into `buf` with truncation; UTF-16 overflow falls back instead of asserting

- **Status: CONFIRMED** · kind: behavioral-difference · severity: low
- Oracle: `src/font/face/freetype.zig:158-183`
- rustty: `crates/rustty-font-shaper/src/face/freetype.rs:322-370` (`copy_into` at `:1461-1466`, `utf16_le_to_utf8` at `:1442-1456`)

The reference returns the FreeType-owned `entry.string` slice directly (full length, regardless of `buf` size) and only uses `buf` for the UTF-16 decode path (where `std.unicode.utf16LeToUtf8` asserts on insufficient output space). The port always copies into `buf` via `copy_into`, truncating a family name longer than `buf.len()`, and its `utf16_le_to_utf8` returns `None` (falling back to the raw bytes) when the UTF-8 output would exceed `buf` instead of asserting. Callers pass 1024-byte buffers, so this only matters for pathological names.

_Evidence:_ `freetype.zig:180` `return string;` vs `freetype.rs:360/365` `return Ok(copy_into(buf, string));` with `let n = std::cmp::min(buf.len(), src.len());`.

_Suggested fix:_ Accept as a lifetime-driven idiom, or return a `Cow` so uncopied names keep full length.

#### S91-F10 — freetype `renderGlyph`/`getMetrics`: log.err/log.warn messages dropped

- **Status: CONFIRMED** · kind: behavioral-difference · severity: low
- Oracle: `src/font/face/freetype.zig:686-695, 710, 1074-1076`
- rustty: `crates/rustty-font-shaper/src/face/freetype.rs:863-872, 887-888, 1283-1284`

Three reference log calls are ported as comments: `log.err("Can't render glyph with unsupported glyph format \"{s}\"")` with the 4-byte tag before returning `UnsupportedGlyphFormat`, `log.warn("glyph={} pixel mode={}")` before `@panic("unsupported pixel mode")`, and the `log.warn("(getMetrics) Width of glyph '水' … discarding ic_width metric.")`. Error/return/panic behaviour is identical; only the log output is missing.

_Suggested fix:_ Wire a logging facade into rustty-font-shaper and emit the same messages.

#### S92-F1 — `OS2.init`: unsupported-version check happens after all field reads

- **Status: CONFIRMED** · kind: behavioral-difference · severity: low (corrected from medium)
- Oracle: `src/font/opentype/os2.zig:351-357, 566`
- rustty: `crates/rustty-font/src/opentype/os2.rs:286-358`

The oracle reads the 16-bit version, seeks back to 0, and dispatches; any version outside 0..=5 returns `error.OS2VersionNotSupported` immediately, before any further byte is read. rustty reads the version, then unconditionally reads the whole v0 prefix plus the version-gated tails, and only then checks `if version > 5`. For a table whose version is e.g. 6 or 0xFFFF but whose data is shorter than the 100-byte v5 layout, rustty returns `InitError::EndOfStream` where the oracle returns `OS2VersionNotSupported`. Both call sites (`coretext.zig:615`, `discovery.zig:778`) only log the error, so the observable effect is log text.

_Evidence:_ `os2.zig:351-357` switch with `else => return error.OS2VersionNotSupported` before any struct read; `os2.rs:286` then `:294-353` reads with `?`, `:355-358` the version guard. PORT NOTE at `:274-278` documents the re-ordered read.

_Suggested fix:_ Move the `if version > 5` check to immediately after reading `version`.

#### S92-F2 — `SVG.init` rejects a record count that does not fully fit in the table

- **Status: CONFIRMED** · kind: behavioral-difference · severity: low (corrected from medium)
- Oracle: `src/font/opentype/svg.zig:41-50, 84-90`
- rustty: `crates/rustty-font/src/opentype/svg.rs:69-79`

The oracle casts `data[reader.seek..]` to a many-item `[*]const [12]u8` and slices `records[0..len]` without a bounds check; it only reads the first 4 bytes of `records[0]` and `records[len-1]` via `glyphRange`. A table whose declared `numEntries` overruns the remaining bytes therefore parses successfully in the oracle. rustty computes `avail = (data.len() - pos) / 12` and returns `InitError::EndOfStream` when `len > avail`, so such a font's SVG table is rejected and `ColorState::init` fails. The rustty file documents this as intentional R6 hardening and pins it with an extra test; it is an observable divergence on malformed-but-oracle-accepted input (where the oracle's own slice extends past the buffer and the binary search would read out of bounds).

_Evidence:_ `svg.zig:42-44` vs `svg.rs:69-72`.

_Suggested fix:_ Either accept the divergence as a deliberate, documented safety guard (record it in the port decision log), or relax the guard to only require that the 4-byte glyph-range prefix of `records[0]` and `records[len-1]` be in bounds.

#### S92-F3 — `SVG.init` with `numEntries == 0` returns EndOfStream in rustty; the oracle hits a u16 underflow safety panic

- **Status: DISPUTED** (refuted by the SEMANTIC lens) · kind: behavioral-difference · severity: low
- Oracle: `src/font/opentype/svg.zig:41-44`
- rustty: `crates/rustty-font/src/opentype/svg.rs:75-77`

With a document list declaring zero entries, the oracle evaluates `records[(len - 1)]` where `len: u16 == 0`, an integer-overflow safety panic (Debug/ReleaseSafe) or UB (ReleaseFast). rustty short-circuits with `if len == 0 { return Err(InitError::EndOfStream) }`. Reported for completeness: the oracle behaviour is a crash on a malformed table and the rustty behaviour is a graceful error, so this is hardening rather than a fidelity regression.

_Suggested fix:_ No change required; document as intentional.

_Refuting lens — SEMANTIC (verbatim):_ "Confirmed the code shapes (svg.zig:44 `records[(len - 1)]` with len:u16 == 0 → integer underflow panic/UB; svg.rs:75-77 `if len == 0 { return Err(InitError::EndOfStream) }`), but the oracle behaviour being replaced is a Zig safety panic / undefined behaviour, not defined observable behaviour. Parity cannot require reproducing a panic-or-UB, no caller can distinguish it from an error result in any defined way, and the finder itself states 'no change required'. Not a functional-parity finding."

#### S92-F4 — `FixedPoint.format`, `OffsetSubtable.format` and `TableRecord.format` have no Display counterpart

- **Status: DISPUTED** (refuted by the SEMANTIC lens) · kind: missing-function · severity: low
- Oracle: `src/font/opentype/sfnt.zig:105-115, 175-196, 209-227`
- rustty: `crates/rustty-font/src/opentype/sfnt.rs:102-201, 330-360`

The oracle defines custom `format` methods: FixedPoint prints its f64 value with `{d}`; OffsetSubtable prints `OffsetSubtable('<tag or 0x00010000>'){ .num_tables = N }`; TableRecord prints `TableRecord("<tag>"){ .checksum = .., .offset = .., .length = .. }`. rustty derives `Debug` only and provides no `Display`, so any `{}`-style logging would format differently. No use site exists in the oracle tree outside `sfnt.zig`, so this is cosmetic.

_Evidence:_ `sfnt.zig:105-115`, `:175-196`, `:209-227`; `sfnt.rs` has only `#[derive(Debug)]` at `:102/:329/:350`; a grep for Display impls in `crates/rustty-font/src/opentype/*.rs` is empty.

_Suggested fix:_ Add `impl fmt::Display` for the three types with the exact reference strings.

_Refuting lens — SEMANTIC (verbatim):_ "Verified the oracle `format` methods exist (sfnt.zig:105-115 FixedPoint, :175-196 OffsetSubtable, :209-227 TableRecord) and that rustty sfnt.rs only derives Debug (:102, :329, :350) with no Display impl anywhere in crates/rustty-font/src/opentype (grep for 'Display|fmt::' hits only glyf.rs error boxing). But `grep -rn \"OffsetSubtable|TableRecord\" /Users/gael/projects/ghostty/src` outside sfnt.zig returns nothing, and the FixedPoint use sites (face/coretext.zig:97, face/freetype.zig:567-568,795) only convert values, never format them — OffsetSubtable/TableRecord are also private nested types of SFNT.Directory. The difference is therefore confined to code no caller reaches, and is purely cosmetic string formatting; per the lens this is refuted."

#### S94-F2 — `Shaper::init` panics on thread-spawn failure instead of returning an error

- **Status: CONFIRMED** · kind: missing-error-path · severity: low (corrected from medium by the presence lens)
- Oracle: `src/font/shaper/coretext.zig:224-229`
- rustty: `crates/rustty-font-shaper/src/shaper/coretext.rs:435-438`

The oracle uses `try std.Thread.spawn(...)` so a spawn failure (EAGAIN/thread limit) propagates as an error from `Shaper.init` to the caller (renderer init fails gracefully). rustty calls `.expect("spawn cf_release thread")`, which panics instead of surfacing `Err` — inconsistent with every other fallible create in the same `init`, which use `.ok_or(Error::OutOfMemory)?`. (The `setName … catch {}` swallow is correctly modelled by `Builder::name`.)

_Suggested fix:_ Map the `io::Error` from `spawn` into the `Error` enum and return `Err` from `init`.

#### S94-F3 — `getFont`: NULL from CTFontDescriptorCreateWithAttributes / CTFontCreateCopyWithAttributes panics instead of returning OutOfMemory

- **Status: CONFIRMED** · kind: missing-error-path · severity: low (corrected from medium)
- Oracle: `src/font/shaper/coretext.zig:602-609`
- rustty: `crates/rustty-font-shaper/src/shaper/coretext.rs:1063, 1071`

The oracle wraps both CoreText creates in `try` (the macos bindings return `error.OutOfMemory` on NULL), so `shape()` returns an error. rustty uses objc2's non-Option `CTFontDescriptor::with_attributes` and `CTFont::copy_with_attributes`, which unwrap internally and abort on NULL. The same file already declares a raw `CTTypesetterCreateLine` precisely to preserve the NULL→OutOfMemory path, and every other CF create in the file is `.ok_or(Error::OutOfMemory)?`, so the treatment is inconsistent.

_Suggested fix:_ Declare the two raw CoreText symbols returning `Option<NonNull<..>>` and map `None` to `Error::OutOfMemory`.

#### S94-F4 — deinit/endFrame diagnostics not emitted (BUG: CFRelease pool was not empty; stop/wakeup notify errors)

- **Status: CONFIRMED** · kind: behavioral-difference · severity: low
- Oracle: `src/font/shaper/coretext.zig:242-252, 259-260, 270-273, 287-292`
- rustty: `crates/rustty-font-shaper/src/shaper/coretext.rs:486-507, 511-563`

The oracle logs `log.warn("BUG: CFRelease pool was not empty, releasing remaining objects")` in non-test builds when `deinit` finds a non-empty pool, `log.err("error notifying cf release thread to stop, may stall err=..")`, and `log.warn("error notifying cf release thread to wake up, may stall err=..")`. rustty preserves these only as comments and emits nothing; the crate wires no logger, so the user-visible warning that indicates a missed `endFrame` is lost. (The "error converting release pool to owned slice" arm is legitimately unreachable in Rust, since `std::mem::take` is infallible.)

_Suggested fix:_ Route these through the workspace log facade, gated on `cfg!(not(test))` for the BUG warning.

#### S95-F1 — `FeatureList.appendFromString` drops the `log.warn` for unparseable feature settings

- **Status: CONFIRMED** · kind: missing-error-path · severity: low
- Oracle: `src/font/shaper/feature.zig:245-259`
- rustty: `crates/rustty-font-shaper/src/shaper/feature.rs:421-436`

The oracle logs `failed to parse font feature setting: "<slice>"` (scoped `.font_shaper`) whenever `Feature.fromReader` returns null for a segment of the comma-separated list. The rustty port keeps the message only as a comment and discards the offending slice (`let _ = i;`), so a user with a typo in `font-feature` gets no diagnostic. Parsing behaviour (silently skipping the bad segment) is otherwise identical. Note the workspace does have an established `eprintln!`-based scoped-log convention inside the font tier, so the PORT NOTE's "no logger wired" premise is weaker than stated.

_Suggested fix:_ Emit `log::warn!("failed to parse font feature setting: \"{}\"", …)` in the else branch; same treatment for the other `log.warn` comment sites in `collection.rs` / `codepoint_resolver.rs`.

#### S97-F2 — wasm `shaper_test` drops its log output and shapes one run per row instead of iterating RunIterator runs

- **Status: CONFIRMED** · kind: behavioral-difference · severity: low
- Oracle: `src/font/shaper/web_canvas.zig:261-304`
- rustty: `crates/rustty-font-shaper/src/shaper/web_canvas.rs:466-479, 585-591`

The oracle `shaper_test` exists to emit diagnostics: on failure `log.warn("error during shaper test err={}")`, and per run `log.info("y={} run={d} shape={any} idx={}")`. The rustty `shaper_test` discards both (`let _ = shaper_test_(...)`, `let _cells = ...`, logs kept as comments) even though `rustty-os`'s wasm log module is a declared dependency of the same feature and is used elsewhere in the crate. It also feeds every non-zero cell of a row into a single run rather than splitting via the run iterator, so the number of `shape()` calls and cell x values differ. The oracle body references deleted APIs (`run.group.group.faceFromIndex`, 3-arg `Terminal.init`, 4-arg `runIterator`), so exact reproduction is impossible; the missing logging is the concrete observable gap.

_Suggested fix:_ Route the warn/info messages through `rustty_os::wasm::log` (scope `font_shaper`) and drive the modern RunIterator over the terminal's RenderState cells.

#### S97-F3 — `testDiffAtlas` omits the on-failure artifacts (test PNG copy into cwd and red/green diff PNG)

- **Status: CONFIRMED** · kind: behavioral-difference · severity: low
- Oracle: `src/font/sprite/Face.zig:287-296, 307-321, 361-405`
- rustty: `crates/rustty-font/src/sprite/face.rs:855-885` (PORT NOTE), `:908-956`

On a reference-open failure or a pixel mismatch the oracle copies the freshly rendered test atlas PNG into the cwd (`sprite_face_test-U+….png`) and, on pixel mismatch, writes a visual diff (`sprite_face_diff-U+….png`, faded gray for matches, reference in R / new in G), naming all three paths in the error log. rustty's `test_diff_atlas` logs only the reference path and writes no files (it instead adds a non-oracle `RUSTTY_REGEN_FONT_KAT_PNGS` regeneration arm). The pass/fail verdicts (bytes-equal, bytes-differ-pixels-match, pixels-differ) are preserved.

_Suggested fix:_ Write the encoded test atlas and a diff PNG next to the reference on failure (the `png` dev-dependency already encodes), and include their paths in the error message.

#### S98-F1 — Stale PORT NOTE in `draw/mod.rs` claims `box.zig` is deferred / only partially carried

- **Status: DISPUTED** (refuted by the SEMANTIC lens) · kind: other · severity: low
- Oracle: `src/font/sprite/draw/box.zig:1-929`
- rustty: `crates/rustty-font/src/sprite/draw/mod.rs:8-11`

The module-root doc comment says "Modules deferred to later batches: box.zig (929 lines) has its own dedicated batch; `r#box` currently carries only the diagonal helpers powerline imports plus arc" and "the large legacy-computing / symbols modules remain in their own batches". `box.rs` is in fact a complete 2519-line port (all 128 dispatch arms, linesChar, dashes, arc, diagonals) and the symbols modules are present and declared in the same file (`mod.rs:21-23`). Doc-only; no functional behaviour differs.

_Suggested fix:_ Rewrite the `mod.rs` header to state that every `draw/*.zig` module is ported; drop the "deferred to later batches" list.

_Refuting lens — SEMANTIC (verbatim):_ "Read mod.rs:1-24 — the stale text is exactly as quoted (lines 8-11). Read box.rs: 2519 lines, single dispatch entry `pub fn draw2500_257F` at box.rs:96 matching oracle box.zig:48 `pub fn draw2500_257F`, no unimplemented!/todo! anywhere in the file (grep returned nothing), and mod.rs:22-23 declares symbols_for_legacy_computing{,_supplement}, whose .rs files exist alongside every other draw/*.zig counterpart (directory listing is 1:1 with the oracle dir). So the comment is factually stale but purely doc-level: no input or state produces a different result, error, or side effect. Under the semantic-equivalence lens this is a documentation-hygiene nit, not an observable functional-parity difference; the 'PORT NOTE deferral' finding class applies to actual deferrals in production code, and there is no deferral here."

#### S100-F2 — `Action.getGObjectType` (GTK boxed GObject type) is a PORT NOTE deferral

- **Status: DISPUTED** (refuted by the SEMANTIC lens) · kind: stub-or-unimplemented · severity: low
- Oracle: `src/input/Binding.zig:998-1008`
- rustty: `crates/rustty-input/src/binding.rs:1386-1398`

The oracle defines `getGObjectType = switch (build_config.app_runtime) { .gtk => gobject.ext.defineBoxed(Action, .{ .name = "GhosttyBindingAction" }), .none => void }`. rustty carries only a PORT NOTE marking it a "deferred apprt seam". This is GTK-only; for the `.none`/macOS runtime the oracle value is `void`, so there is no observable behaviour on the macOS-first target. Recorded for traceability so the GTK sub-project picks it up.

_Suggested fix:_ When the GTK apprt lands, register the boxed GType named `GhosttyBindingAction` for `Action` in the GTK binding crate.

_Refuting lens — SEMANTIC (verbatim):_ "The use-site the oracle's getGObjectType exists for IS ported. Oracle Binding.zig:998-1005 defines the boxed GType only so the Action can cross a GObject boundary; the sole consumer in the oracle GTK tree is command_palette.zig:54-64, whose `trigger` signal carries *const input.Binding.Action (grep for GhosttyBindingAction across src/apprt/gtk returns only the defineBoxed site — nothing, including .blp templates, looks the type up by name). rustty covers this at crates/rustty-apprt/src/gtk/class/command_palette.rs:63-72 with `#[derive(glib::Boxed)] #[boxed_type(name = \"GhosttyCommandPaletteAction\", nullable)] pub struct ActionBoxed(pub binding::Action)`, emitted at :351 `self.emit_by_name::<()>(\"trigger\", &[&ActionBoxed(action)])` and registered in the signal at :160. This is the expected R4 relocation (rustty-input cannot depend on glib). The only residue is the GType's registered name string, which nothing reads — not observable. For the .none runtime the oracle value is `void`, so no difference there either."

### Unverified findings (over the verification cap)

None — every finding in this area received both verification lenses.

## Performance-only differences (not findings)

- **Hashing:** `DescriptorCache` (`HashMapUnmanaged(Descriptor, ?Index)` with a Wyhash-comparing context) becomes `HashMap<u64, Option<Index>>` keyed by `Descriptor::hashcode()`; `SharedGrid.glyphs`, `SharedGridSet.Map`, `Key.hashcode`, `run.zig`'s run hash and `Binding.Action::hash` all use std `DefaultHasher` (SipHash) instead of Wyhash(0). Key-equality semantics ("same hashcode ⇒ same entry") are preserved and no hash value is serialized, compared across implementations, or asserted by any test.
- **Container idioms:** `SegmentedList(EntryOrAlias, 0)` → `Vec<Box<EntryOrAlias>>` (Box preserves the `alias: *Entry` pointer stability); `std.ArrayListUnmanaged` → `Vec` with `clear()` for `clearRetainingCapacity()`; `ensureTotalCapacity`/`ensureUnusedCapacity` → `Vec::reserve`; SFNT records / glyf Outline own `Vec`s instead of allocator-owned slices; `ArenaAllocator`s in `DerivedConfig`/`Key`/coretext `shape()` replaced by owned `Vec`s with no-op `deinit`.
- **Comptime flattening:** `Modifier.apply(anytype)` → three per-type methods; `Minimums` clamp and `Metrics.Key` hand-unrolled over the same 12/17 fields; the sprite `ranges` table and `getDrawFn`'s `inline for` hand-materialized (verified identical set/order/no-overlap) with the overlap `@compileError` becoming a unit test; `Action::parse`'s `inline for` becomes a byte-string match with per-type macros; the octant/mosaic comptime tables become const fns over the same data; Zig packed structs (`Lines(u8)`, `Quads(u4)`, `BranchNode(u5)`, braille `Pattern(u8)`, `Sextants(u6)`, `SmoothMosaic(u10)`) become plain fields with explicit bit extraction.
- **Ownership / RAII:** `CFRetained<T>` Drop replaces manual `defer`/`errdefer release()`; `CoreTextData`/`WindowsData`/`WebCanvasData` drop their `alloc` fields; `set_variations` wraps its body in a closure so `done_mm_var` runs on every exit; the discovery iterator gains a `Drop` + idempotent `deinit` materialising the oracle's `defer disco_it.deinit()`; `Cache`'s owned slice is a Copy fat raw pointer freed in the `evicted` hook.
- **z2d / rasterizer:** `glyf_rasterize` draws into a z2d-owned `Vec<Alpha8>` moved out via `Vec::from_raw_parts` instead of `initBuffer` over a caller buffer; rustty-z2d is allocator-free so `flipHorizontal`/`flipVertical`/`innerStrokePath`/`close_path`/`move_to` lose their `Allocator.Error` returns; `canvas.writeAtlas` collects into a temporary `Vec<u8>` before `setFromLarger`.
- **Structure-preserving rewrites:** `scaleFactor`'s labeled switch/continue is a loop over a mutable adjustment (same fall-through order); `Collection.getIndex` uses `iter().enumerate()`; `sort_matching_descriptors` precomputes each Score once (both sides unstable-sort, so tie order is unspecified in both); both non-LTR cell sorts are stable on both sides; `branchNode` hoists four `canvas.box` writes above `getContext()` with byte-identical surface-mutation order.
- **String/FFI handling:** CFString reads use `to_string()` instead of `CFStringGetCString` into stack buffers (overflow arms reproduced by length checks); the CF_INLINE surrogate-pair helper is reproduced as a local Rust fn; `zig-js` dynamic JS calls become a fixed `rustty_wc_*` extern-C handle table; base64 and PNG codecs swapped for an inline decoder and the `png` crate (payloads verified byte-identical, reference PNG decodes to an identical pixel plane).
- **Misc:** `u2`/`u21` modelled as `u8`/`u32` (all values in range); the coretext `ColorState` re-parses the stored `SVG ` bytes per call; `Face::copy_table` ignores the passed allocator; the shaper mpsc stand-in is unbounded where the oracle mailbox is a 64-slot blocking queue; `render_glyph`'s scratch buffer is zero-initialised where the reference leaves it uninitialised (stb overwrites every byte); the web_canvas `cell_buf` is a growable `Vec` instead of a caller-provided fixed slice; `grapheme_glyph_index` copies the cluster key instead of storing the caller's pointer.

## Refuted candidates

- **S87-F5 — rasterize: width > i32::MAX yields `Err(Surface(InvalidWidth))` instead of the oracle's `@intCast` safety panic.** Refuted by both lenses: to reach the cast, width must saturate to ~u32::MAX, and the oracle's `alloc.alloc(u8, width*height)` returns `error.OutOfMemory` first, so both sides fail with an error for the same absurd input.
- **S90-F1 — `init_font` always builds a HarfBuzz font (`HARFBUZZ_SHAPER` hard-coded true) although the macOS backend is `coretext`.** Refuted by both lenses: `hb_font` is read only by `shaper/harfbuzz.rs`, so the macOS-default coretext shaper's output is unchanged; the rustty face is exactly the oracle's `.coretext_harfbuzz` face, retained so the HarfBuzz shaper's 25-test port can run on macOS. Residue is a self-contradicting PORT NOTE comment.
- **S91-F1 — web_canvas `calcMetrics`: hand-built Metrics replaced by FaceMetrics derived via `Metrics.calc`.** Refuted by both lenses: the oracle's `font.Metrics{…}` literal omits nine required non-default fields and cannot compile at the pin; the modern contract is `getMetrics() -> FaceMetrics` + shared `Metrics.calc`, which the port follows. The `px_per_em` sub-claim is also wrong (the context is set with `{points}px`).
- **S91-F4 — web_canvas `renderGlyph`: 4-byte atlas arm swizzles RGBA→BGRA instead of storing Canvas bytes verbatim.** Refuted by both lenses: the oracle's `.rgba` arm references an Atlas format variant that does not exist at the pin, so it cannot compile; the port maps the only 4-byte format that exists and preserves the grayscale arm byte-for-byte. The finder recommended no code change.
- **S96-F1 — `debugPositions` is preserved only as a comment, not compiled.** Refuted by both lenses: the oracle's only call site is itself commented out, so the function emits nothing at runtime in either tree; rustty preserves the full body verbatim plus the commented call site, matching the oracle line for line.
