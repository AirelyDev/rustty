# rustty Next-Phase Plan (post crown-jewel)

Crown jewel (rustty-vt, all of terminal/) DONE — 1961 tests, gate-green. 12 crates
ported (~2197 tests); 18 stubs. This sequences the rest.

## Stage A — complete the crown jewel (mechanical + 1 decision)
1. **SIMD wiring** [MECHANICAL, do first] — rustty-simd is ALREADY ported; just wire
   stream.rs:651 todo!() + nextSliceCapped/consumeAllEscapes/consumeUntilGround +
   the nextSlice SIMD arm (ref stream.zig:471-577, ~120 loc). Closes ~23 G3b + the
   stream SIMD deferral. Scalar fallback stays default; SIMD arm under `simd` feature.
2. **glyf + Glyph pure parsers** [MECHANICAL, no C libs] — font/opentype/glyf.zig (1046
   loc, 14t) + font/Glyph.zig (630 loc) into rustty-font; pure OpenType parsing, only
   std + siblings. Unblocks apc. (embedded-font byte tables port too.)
3. **apc cluster** (66t) — needs glyf. **[DECISION D1]** break the vt->font cycle (font
   already deps vt). RECOMMENDED: a `GlyfDecoder` trait seam in vt (matches the proven
   sys.rs `decode_png` RwLock-hook pattern — pre-authorized in PORT_PLAN/DEFERRALS),
   keeping apc inside vt + the 2001-test contract self-contained. Alt: hoist apc above
   vt+font. apc/glyph/response (12t, leaf, no font) can land first.

## Stage B — vt-c C-ABI artifact — ✅ DONE (2026-06-25)
rustty-vt-c COMPLETE: 308 tests, 156 non-WASM rustty_* exports, cbindgen header +
C/C++ compile, ABI const-asserts pinned, D3 trampoline seam (offset_of! container_of)
Miri-clean (SB+TB) + ASan-clean, vt untouched (2019). D2 cbindgen + D3 trampoline both
signed off. (stream.Action [16]u64 c_union stays deferred — not a c/ export type.)

## Stage B — vt-c C-ABI artifact [MECHANICAL + 1 decision]
4. **rustty-vt-c** (terminal/c, 28 files, 13.7k loc, 307t) — absorbs the deferred
   ScrollViewport/stream c_union C-ABI. Rust enums are source-of-truth; vt-c is FFI
   projection. **[DECISION D2]** cbindgen tagged-union layout + the `[16]u64`/`[2]u64`
   ABI-size verification ("verify before shipping ABI-compatible libghostty" — load-bearing).

## Stage C — cheap mid-tier leaves [MECHANICAL] (only ported deps)
5. cfg-fmt (9t), args (64t, breaks config<->cli), crash (9t, sentry-sys), terminfo (4t, emits "rustty").

## Config cluster — C1 ✅, C2 ✅, C3 ✅ DONE (2026-06-26) — CLUSTER COMPLETE
C1 leaf value-types (36t) + C2 the giant Config.zig (10934loc, 113 tests, 203 fields
== 203 Key variants — field-parity audit caught 7 dropped fields incl font-size; the
4 comptime passes via Target-trait + exhaustiveness test; load/finalize/parseManuallyHook/
Replay/theme machinery). C3 = CApi.zig(8t) + c_get.zig(6t) — the config C-ABI
(rustty_config_new/free/clone/load_*/finalize/get/trigger/diagnostics + the typed
config_get over the Key registry); CApi stays IN rustty-config (NOT a -c crate);
R2 identity exports rustty_config_* via #[no_mangle] extern "C". 183 config tests
(incl. 14 C3), whole count-gate GREEN, no workspace regression (vt 2019). C3 verified:
Miri 14 isolated (c_api 8 + c_get 6) 0 UB/leak + ASan c_api 0 findings + G3b CApi
13/c_get 12 distinctive comments, missing=0 + clippy/fmt/acyclic green. Every
config/*.zig now mapped in check_test_counts.py (incl. formatter_file + Wasm).
Deferred (documented): RepeatableCodepointMap (font/CodepointMap.zig unported),
FileFormatter docs (help_strings), url regex (oniguruma-sys), Wasm.zig browser
exports (feature-gated OFF, mirrors vt wasm deferral). The renderer/apprt/cli/
termio tier is now UNBLOCKED (config was the last shared dep for those modules).

## Stage F0 — font-pure tier: DONE
The rustty-font PURE tier is complete (F0): opentype/{glyf,sfnt,head} + Metrics +
Atlas + CodepointMap + glyf_rasterize (z2d-backed, PNG known-answer pixel-exact) +
the WHOLE sprite tier — sprite.zig root + sprite/{canvas,Face} + draw/{block,box,
braille,branch,common,geometric_shapes,powerline,special,symbols_for_legacy_computing
(+supplement)}. check_test_counts.py carries the font_sprite* mappings (draw modules
0-test by design → gated by G3b comment-coverage, missing=0). Face.zig "render all
sprites" + bubbletea glyph-protocol PNG compares are pixel-exact vs res/*.png.
rustty-font stays PURE & BELOW vt (no rustty-vt edge; G7 asserts it). Miri clean,
clippy/fmt clean. NEXT = F1 (the -sys/face tier below).

## Stage F1 — font -sys tier: freetype-sys DONE
`crates-sys/freetype-sys` vendors FreeType **2.13.2** (the exact release the
reference pins in pkg/freetype/build.zig.zon) under `vendor/` and builds it
standalone via `cc` + `bindgen` — R3 clean (no ghostty build-time ref; the sole
`/Users/gael/projects/ghostty` string is a doc-comment). This is the FIRST
bindgen -sys and it VALIDATES the -sys-at-scale template:
  * `vendor/` = full upstream src tree + `include/` + `LICENSE.TXT`+`docs/{FTL,GPLv2}.TXT`.
  * `build.rs`: `cc::Build` over the verbatim `SOURCES` module list from the
    reference `buildLib`; same flags (`-DFT2_BUILD_LIBRARY
    -DFT_CONFIG_OPTION_SYSTEM_ZLIB=1 -fno-sanitize=undefined`, `-DHAVE_UNISTD_H
    -DHAVE_FCNTL_H` off-Windows, `-fPIC` on freebsd/musl), `.warnings(false)`.
  * per-OS `ftsystem.c`/`ftdebug.c` selection (linux=builds/unix, windows=
    builds/windows, else=src/base) mirrors the reference switch. macOS uses the
    generic `src/base/ftsystem.c` — cc resolves system headers natively (no
    apple-sdk path shim needed as in the Zig build). Cross-platform: config is
    `-D`-driven (ftoption.h keeps SYSTEM_ZLIB commented, we define it), no
    macOS-only assumptions.
  * system-zlib wired to `zlib-sys`: header via `DEP_Z_INCLUDE`, static `z`
    archive kept on the link line by `extern crate zlib_sys;` (rustc would
    otherwise prune it + drop the ftgzip inflate symbols). `cargo tree -e normal`
    = freetype-sys → zlib-sys ONLY (G7 acyclic).
  * bindgen over `wrapper.h` (same headers as reference c.zig) → 4054-line
    bindings; libclang = Xcode `libclang.dylib`. KAT (`kat_freetype_2_13_2_
    memory_face`): FT_Init → version==(2,13,2) → FT_New_Memory_Face on embedded
    GeistMono → num_glyphs==825, units_per_EM==1000, SCALABLE flag → Done. GREEN.
  * `cargo:include` re-exports the FreeType include dir as `DEP_FREETYPE_INCLUDE`
    for downstream (harfbuzz-sys/rustty-font).
REUSABLE for NEXT: **harfbuzz-sys → freetype** (consume `DEP_FREETYPE_INCLUDE`
for hb-ft; same vendor+cc+bindgen+KAT template), **fontconfig-sys → freetype +
libxml2** (add a libxml2-sys vendor first, then wire both include dirs).

## Stage F2 — font -sys tier: harfbuzz-sys DONE
`crates-sys/harfbuzz-sys` vendors HarfBuzz **11.0.0** (the exact release the
reference pins in pkg/harfbuzz/build.zig.zon — the
`deps.files.ghostty.org/harfbuzz-11.0.0.tar.xz` mirror) under `vendor/` and
builds it standalone via `cc` (C++17) + `bindgen` — R3 clean (no ghostty
build-time ref; the sole `/Users/gael/projects/ghostty` string is a
doc-comment). Applies the validated -sys template as a C++ variant:
  * `vendor/` = full upstream `src/` tree + `AUTHORS` + `COPYING` (Old-MIT license).
  * `build.rs`: `cc::Build.cpp(true).std("c++17")` over the reference `buildLib`'s
    ONE amalgamation TU `src/harfbuzz.cc` (which `#include`s the whole `src/`
    tree), include path = `src`; same `-D` flags (`-DHAVE_STDBOOL_H`; off-Windows
    `-DHAVE_UNISTD_H -DHAVE_SYS_MMAN_H -DHAVE_PTHREAD=1`; `-DHAVE_FREETYPE=1` +
    the four `HAVE_FT_*` capability flags the reference hard-enables), CoreText
    OFF (reference default `enable-coretext=false`), `.warnings(false)`/`-w`,
    MSVC ubsan-off guard mirrors the reference.
  * FreeType wired to `freetype-sys` (the reference's `lib.linkLibrary(freetype)`):
    headers via `DEP_FREETYPE_INCLUDE` (for `hb-ft.cc`/`hb-ft.h`), the static FT
    archive kept on the link line by `extern crate freetype_sys;` (one-version
    linking). `links = "harfbuzz"` enforces single linkage.
  * bindgen over `wrapper.h` (`hb.h` + `hb-ft.h`, mirroring reference c.zig
    @cImports; hb-coretext.h omitted since coretext is off). `[lib] doctest=false`
    (HB doc-comments are not Rust).
  * KAT (`kat_harfbuzz_11_0_0_shape_hello`): `hb_version`==(11,0,0) → build an
    `hb_font_t` from an embedded GeistMono FT_Face (`hb_ft_font_create_referenced`,
    the `-DHAVE_FREETYPE=1` bridge) → buffer add-utf8 "hello" + set LTR/Latin/"en"
    → `hb_shape` → assert 5 glyphs, all non-.notdef, monospace equal +x_advance,
    zero y_advance. GREEN — proves HB links AND shapes, and that the FT bridge
    links a single static FT archive.
  * `cargo:include` re-exports the HarfBuzz `src` include dir as
    `DEP_HARFBUZZ_INCLUDE` for downstream (rustty-font).
  * `cargo tree -e no-dev` = harfbuzz-sys → freetype-sys → zlib-sys (G7 acyclic,
    check_acyclic GREEN). clippy/fmt clean (generated bindings allow-gated).
**F3 faces now UNBLOCKED**: freetype + harfbuzz both done. Remaining face deps:
CoreText via `objc2`/`objc2-core-text` crates (macOS-native, no vendored -sys);
fontconfig/libxml2 -sys deferred to the Linux font-discovery milestone.

## Stage F3 — font faces + discovery tier: ✅ DONE (2026-07-01)
New crate **rustty-font-shaper** (the D1 3-tier split: ABOVE vt+font; rustty-font
STAYS pure below vt, acyclic gate confirms no rustty-font→vt edge). The impure
C-lib/CoreText face+collection+discovery tier: face/freetype.rs (9t, FreeType Face
over freetype-sys) + face/coretext.rs (8t, CoreText Face over objc2-core-text) +
face/mod.rs (3t, backend-selecting facade) + collection.rs (13t) + deferred_face.rs
(2t) + codepoint_resolver.rs (3t) + discovery.rs (8t, coretext discoverer +
fontconfig/windows skip-stubs). 47 tests pass; ASan clean; clippy/fmt/acyclic green;
whole check_test_counts.py gate GREEN (7 font_shaper_* modules mapped, rust>=zig);
scoped G3b missing=0; no workspace regression (vt 2019, font 74). Backend matrix
cfg-gated (macOS default=coretext, Linux default=fontconfig_freetype). DEFERRED
(documented, DEFERRALS.md): fontconfig discoverer (Linux, ported as SkipZigTest-style
skip-stub #[test]s guarded on BACKEND==FontconfigFreetype), windows/DirectWrite
discoverer (skip-stub), web_canvas face (wasm, feature-gated OFF).
NEXT = **F4 shaper** (shape.zig + shaper/{coretext,harfbuzz,noop,run}.zig — the
HarfBuzz/CoreText text-shaping tier over the F3 faces).

## Stage F4 — font shaper tier: ✅ DONE (2026-07-01) — rustty-font-shaper COMPLETE
The text-shaping tier over the F3 faces. shape.rs `Shaper` selector wired from the
`noop` placeholder to the reference default backend (`font/backend.zig`
`Backend.default`): cfg split → macOS lands on `coretext::Shaper` (CoreText-native),
every other target on `harfbuzz::Shaper`; the `noop` shaper stays built+tested via its
own submodule. shaper/harfbuzz.rs+harfbuzz_tests.rs (24 tests: 20 executing + 4
SkipZigTest-style) over hb.rs (RunIteratorHook prepare/addCodepoint/finalize +
glyph-run/ligature cell_offset extraction). shaper/coretext.rs+coretext_tests.rs
(27 tests, live CoreText via objc2 CTLine/CTRun/CTTypesetter/CTStringAttributes +
the cf_release_thread raw-ptr Box<Thread>). check_test_counts.py extended:
`font_shaper_coretext` now maps coretext.rs + coretext_tests.rs (mirrors the
harfbuzz.rs+harfbuzz_tests.rs split). 110 rustty-font-shaper tests pass (59 F3 + 24
harfbuzz + 27 coretext); ASan clean (FFI tests can't run under Miri, ASan covers
harfbuzz/CoreText); clippy -D/fmt/acyclic green (rustty-font STILL no vt edge); whole
count gate GREEN (count_ok); scoped G3b harfbuzz.zig+coretext.zig missing=0; no
workspace regression (vt 2019, font 74).
**renderer now UNBLOCKED**: consumes SharedGrid (full init/deinit/RwLock/metrics/atlas
grids) + the `Shaper` selector.

## Stage R-a — renderer headless slice: ✅ DONE (2026-07-01) — rustty-renderer R-a
The unit-portable, GPU-free renderer leaves (memory heavy-tier-strategy R-a). Ported:
cell.rs (renderer/cell.zig, 5 tests — the per-cell `CellText` fg-glyph + `CellBg`
vertex assembly from the Screen + font SharedGrid: `Contents` add/clear/setCursor/
getCursorGlyph, constraintWidth, symbol/graphics-element classification), cursor.rs
(renderer/cursor.zig, 4 tests — the renderer `Style` + `style()` guard chain), state.rs
(renderer/State.zig, 2 tests — the render `State`/`Preedit` range/width), and shaders.rs
(the CPU-side `CellText`/`CellBg`/`Uniforms` repr(C) vertex-data structs from
renderer/opengl/shaders.zig that cell.zig fills — struct defs only, NOT the GPU
pipeline machinery). renderer/size.zig (6 tests) already lives in the rustty-render-types
leaf (A1 hoist) and is re-exported. check_test_counts.py extended: renderer_cell (5/5),
renderer_cursor (4/4), renderer_state (2/2), renderer_size (6/6 → render-types lib.rs),
renderer_link (3, apc_deferred=3). 11 rustty-renderer tests pass; Miri clean (11 pass, 0
UB/leak — headless, pure vt path); clippy -D/fmt/acyclic green (rustty-font STILL no vt
edge; font-shaper edge acyclic); whole count gate GREEN (count_ok); scoped G3b
cell.zig+cursor.zig+State.zig missing=0; no workspace regression (vt 2019, font-shaper 110).
**DEFERRED**: renderer/link.zig (3 tests — link/hover highlight rects) blocked on
Oniguruma (`oni.Regex` unported tree-wide; `Set.fromConfig` compiles a regex,
`renderCellMap` runs `regex.search`), a SkipZigTest-style defer mirroring config
url-regex + input Link.zig (DEFERRALS.md). Lands with oniguruma-sys.
**NEXT (heavy tier, sequenced)**: R-b/R-c — the shader/image tier (renderer/shaders/,
image.zig, shadertoy custom shaders) → R-d — the GPU draw path (Metal/OpenGL/WebGL
backends, generic.zig, glslang/spirv-cross/wuffs/libpng/zlib-sys). On the embedded-first
milestone path, **apprt-api** (D5 partition) is the next headless deliverable.

## Stage D — heavy tier (later)
rest of font (faces/shapers: freetype/harfbuzz/fontconfig/CoreText -sys, ~10.7k loc) →
input (completes key_mods once config lands) → config (oniguruma-sys) → renderer
(Metal/OpenGL/WebGL, glslang/spirv-cross/wuffs/libpng) → termio → core → apprt
(25.9k loc, ~80% GTK/objc/wayland — biggest non-vt module, weakly test-gated) → cli → bin.

## Decision points (need human)
- **D1** apc cycle-break: GlyfDecoder-trait-in-vt (recommended) vs hoist apc. (pre-authorized both)
- **D2** vt-c cbindgen ABI layout + [16]u64 size verification.
- D3 A1 render-types size-hoist sign-off (already implemented). D4 renderer SCC trait-breaks. D5 apprt-api partition.

## Risks (largest first)
1. apprt-GTK (25.9k loc platform/GUI, weak test-gate). 2. font C-stack (freetype/harfbuzz/CoreText, vendored -sys). 3. C-ABI byte-exact compat. 4. -sys crates (oniguruma/glslang/wuffs/dcimgui/sentry). 5. apc cycle integrity (mitigated by the decode_png seam template).

## Stages A–C are almost entirely mechanical, strong test gates, only 1 new -sys (sentry). The project-scale risk is Stage D (apprt-GTK + font/renderer C stacks).
