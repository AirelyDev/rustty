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
