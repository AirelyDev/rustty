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

## Stage R-b/R-c + oniguruma + apprt-api + -sys backlog: ✅ DONE (2026-07-01)
- **oniguruma**: oniguruma-sys (6.9.9, generated config.h) + rustty-oniguruma (`oni` safe wrapper) + vt `oniguruma` feature wired. Closed 4 deferrals: vt StringMap (3 tests), config url regex (90 cases), input Link, renderer link. `onig_initialize` made thread-safe via OnceLock (see below).
- **renderer R-b/R-c**: shadertoy.rs (4t, glslang+spirv-cross FFI), link.rs (3t, oniguruma), + renderer::Health (1t). renderer now 19 tests. R-d GPU (Metal/OpenGL/generic.zig, image.zig) = Phase 3, no unit gate.
- **apprt-api KEYSTONE** (D5): rustty-apprt-api filled — action/surface/structs/runtime/ipc, 20 tests, the `App`/`Surface`/`CoreSurfaceHandle`/`Target` GENERIC (not dyn) trait seam that BREAKS core↔apprt (acyclic green). PORT_PLAN A5 resolved (apprt-api 20 / apprt 47→27, global 3234 unchanged). Deferred (additive): gtk `getGObjectType` GObject impls → apprt gtk backend; CValue byte-exact C-ABI pinning → embedded milestone (vt-c D2 pattern); newConfig + mailbox Timeout/Size → land with core.
- **-sys**: wuffs/glslang/spirv-cross (renderer) + libpng/sentry/dcimgui (backlog) all vendored + KAT-green. dcimgui uses freetype-sys's bundled libpng (no dup). **DEFERRED: fontconfig-sys + libxml2-sys** (new-dep pair; Linux font discovery; off the macOS embedding critical path).
- **MEMORY-SAFETY FIX**: the torn-`BufNode.data` corruption under parallel `cargo test` was root-caused (NOT the arena — a downstream victim) to concurrent non-thread-safe `onig_initialize()` (reference `ensureInit` never wrote its `initialized` flag → re-init every call). Fixed with OnceLock; test-serialization lock removed; 0/60 stable at 48 threads. See DEFERRALS.md + memory parallel-cargo-test-exposes-c-init-races.

## INTEGRATION TIER prerequisites: ✅ DONE (2026-07-02) — crash + termio + Command
Per INTEGRATION_TIER_BLUEPRINT.md, 4 commits (8f1fd89, e55d5b4, 6c46a0a, 61b8a52):
- **crash**: rustty-crash filled (dir/sentry/sentry_envelope/main + pkg/sentry over sentry-sys) — 12 tests (9 owed + 3 extra), ThreadStateSurface seam as designed (tuples, live *const read), ASan green.
- **termio**: rustty-termio filled — **44 tests** (Exec 10 + shell_integration 21 + message 2 + root 1 + Command 10). Command relocated per break #13b (PORT_PLAN crate 23/24 corrected). renderer gained message.rs + Thread::Mailbox (BlockingQueue, no xev/GPU). stream_handler = full vt Handler impl.
- **D6 deferrals** (all 0-test, verbatim bodies, DEFERRALS.md): xev event-loop wiring (Thread run/callbacks, Termio threadEnter/processOutput, Exec Subprocess.start arena self-ref, ReadThread), mailbox::send mutex pattern, surfaceMessageWriter/rendererMessageWriter (opaque core Timeout).
Workspace ≈3282+ tests green.

## CORE (Surface/App/surface_mouse): ✅ DONE (2026-07-02) — rustty-core COMPLETE
Per CORE_BLUEPRINT.md (adversarially reviewed ACCEPT-WITH-FIXES), landed in 18
chunks C0→C15 + C-T + C-F. All of **Surface.zig (6036) / App.zig (604) /
surface_mouse.zig (336)** ported LIVE against the placeholder leaves — GPU
Renderer / xev handles / uninhabited Inspector are the ONLY `unimplemented!()`
production leaves (dependency placeholders, no reference logic stubbed), so ~95%
compiles+reviews as real Rust; runtime panics are confined to those leaves.
- **Tests: 15** (R1-owed **1** = surface_mouse `keyToMouseShape`, the sole
  reference `test{}` across the 3 files). +14 rustty-only glue (§7): app.rs `mod
  test_runtime` **13** App/mailbox seam tests (TestApp/TestSurface fake) + lib.rs
  **1** `type_core_prototype` (the C0 self-referential `Core = Surface<A>` bound
  proof). `Surface<A>` is untested-by-construction (init spawns pty/renderer/io
  threads; ghostty has 0 Surface tests) ⇒ **G3b comment-coverage + per-chunk
  review are the gate** — full-file G3b missing=0 (Surface.zig 1000 / App.zig 130
  / surface_mouse.zig 39 distinctive comments, all matched).
- **Design questions RESOLVED:** apprt-api `A: App` genericity threads once and
  stops at the mailbox seam (§1); the growth-point `App`/`Surface` trait methods
  filled (§2); App::Mailbox queue types = concrete `datastruct::blocking_queue::
  {Timeout, Size}` (§3, resolves apprt-api's opaque assoc types); inspector seam
  = pure-data `Event`/`EventRing` ported + `Inspector` uninhabited placeholder
  (§4); renderer-thread/GPU + xev paths = core-local `renderer_seam.rs`
  dependency placeholders (§5, dodges two Cargo cycles); `type Core` = OPTION 1
  self-referential associated-type bound (C0 DECISION, trait solver accepts).
- **Seams landed for lower tiers:** shared `rustty-xev` crate (D6 home);
  guard-less `rustty_zigstd::thread::Mutex` (renderer `State.mutex` retyped);
  crash `ThreadStateSurface for Surface<A>`; C-T un-defers the termio
  mutex-family + `apprt.surface.newConfig` (`app::new_config`).
- **Gates:** build + all crate tests + clippy `-D warnings` (workspace) + fmt +
  acyclic (G7, core below apprt) + test-counts (G-gate) all GREEN; no workspace
  regression. **OPEN deferrals** (all 0-test, R1-safe — DEFERRALS.md "rustty-core
  C-F" is the consolidated ledger): activate/deactivateInspector (Phase-3
  inspector), performBindingAction `.search` arm (search-thread mutex/spawn
  unification), renderer_seam GPU/xev leaves + surface.rs xev `notify()` leaves
  (Phase-3 R-d + D6 libxev), App.zig Wasm block (wasm feature).
- **NEXT: apprt/embedded** — the libghostty embedding milestone (embedded.zig
  2250 loc, 71 C-ABI exports via vt-c cbindgen + pinned-padding + const-size
  assert) + none.zig headless; then cli + bin. core now fills apprt-api's
  growth-point trait methods, so the embedded backend implements a known seam.

## EMBEDDED C-ABI (apprt/embedded): ✅ DONE (2026-07-02) — libghostty C ABI 71/71
Per EMBEDDED_BLUEPRINT.md (adversarially reviewed ACCEPT-WITH-FIXES, 9 fixes
folded), landed in 12 chunks E0→E11. **embedded.zig (2250) + none.zig (19) +
browser.zig (4) + the `main_c.zig` C-API hub** ported: the concrete `embedded`
backend (App/Surface impl apprt-api's generic traits — the FIRST concrete `A`),
the `none`/`browser` module stubs, and the **71 `ghostty_*` (`rustty_*`) C-ABI
exports** in the new **`rustty-apprt-c`** top-sink crate (cdylib/staticlib/rlib →
`librustty.{dylib,a}` + `include/rustty-apprt.h`).
- **C-ABI: 71/71 compile + link + header-generate NOW.** cbindgen (E11) emits
  `include/rustty-apprt.h` (via `xtask header-gen-apprt`); it compiles standalone
  as **C AND C++** (`cpp_compat`, clang `-std=c11`/`-std=c++17 -Wall -Wextra`).
  Cross-crate C types hand-pinned in `cbindgen.toml` `after_includes`
  (`parse_deps=false`, vt-c precedent) — Options(64)/SurfaceOptions(88)/EnvVar(16)/
  Platform.C(8)/TargetC(16)/ActionC(32) + the input `Key`(176)/`Action`/mouse/
  split enums — gated by D2 const-asserts in `capi.rs` (drift fails the build).
- **Smoke KAT (E11, the "step up" from vt-c's header-only gate):** `tests/smoke.c`
  is a REAL C host — `rustty_init` → config C-ABI (`rustty_config_new`/`_finalize`)
  → `rustty_surface_config_new` → `rustty_app_new` with REAL typed `extern "C"`
  callback stubs (the fn-ptr-ABI backstop, ADV-FIX 7) → `rustty_config_key_is_binding`
  → `rustty_app_needs_confirm_quit` → free. `tests/smoke.rs` shells cc/clang to
  compile+link (against the self-contained cdylib — the fat staticlib does not
  bundle the vendored `-sys` C archives) + run, asserting **exit 0**. Does NOT
  call `rustty_surface_new`/`_set_window_background_blur` (the leaf boundary).
- **Runtime-live coverage ≈ 21% (15/71): the ~14 app/config/pure-marshal exports.**
  The GPU/xev/objc/coretext half links now and PANICS when driven (documented
  Phase-3 seam). The runtime boundary is `rustty_surface_new` → core `Surface::init`
  → `renderer_seam::{Renderer,Thread}` (Phase-3 R-d + D6). apprt-c has exactly **4**
  `unimplemented!()` code-leaves, all E10 macOS/objc/CGS/renderer/coretext
  (`set_window_background_blur`/`surface_set_display_id`/`quicklook_font`/
  `quicklook_word`); the inspector/metal exports delegate to the E4 backend leaves.
  E11 creates NO new leaf.
- **Gates:** workspace build clean + apprt-c tests (smoke KAT) + adjacent
  regressions (apprt-api 20 / core 15 / crash 12 / renderer 19 / termio 44) +
  clippy `-D warnings` + fmt + acyclic (apprt-c a clean top-sink) + test-counts
  (embedded/none/browser 0-test rows GREEN) + full-file scoped-G3b
  (embedded.zig 235 / none.zig 1 / browser.zig 0 distinctive, missing=0) all GREEN.
- **NEXT: cli + bin** (the `rustty` binary over the `none` backend, headless
  first — EMBEDDED_BLUEPRINT §1 rec 2), then Phase-3 (renderer R-d + D6 xev +
  the objc/Metal/dcimgui inspector) makes the live-surface half runtime-real.

## CLI + BIN: ✅ DONE (2026-07-02) — the headless `rustty` product RUNS
Per CLI_BLUEPRINT.md (adversarially reviewed ACCEPT-WITH-FIXES, 6 ADV-FIXES
folded), landed in 6 chunks C1→C6. **`src/cli/` (dispatcher + 18 action impls +
ssh subsystem) → `rustty-cli`; `src/main.zig` + `main_ghostty.zig` → the `rustty`
bin.**
- **cli = 44 tests now + 1 deferred (explain_config, ADV-FIX 3).** C1 dispatcher
  (action 7 / ghostty 4 + help + version-PARTIAL + cli.zig re-export); C2 pure
  config/introspection actions + Pager (2); C3 ssh subsystem (Entry 3 / DiskCache
  12 / ssh_cache 5 / ssh 10 = 30); C4 apprt-backed + misc (crash_report /
  edit_config / the none-backed IPC new_window / toggle_quick_terminal exit-1 /
  tui seam); C5 the vaxis/zf-touching actions' plain/enumeration paths LIVE
  (list_colors / list_keybinds --plain + list_themes enumeration; boo + the
  interactive pickers are vaxis-deferred leaves). No cli test needs a live app.
- **bin = C6 (ADV-FIX 2/6).** `main.rs` (entrypoint selector) + `main_ghostty.rs`
  (`main()` dispatch + `logFn` + `std_options`). Action DETECTION lives in the BIN
  (`cli::action::detect_args::<cli::ghostty::Action>` over os args → the bin's own
  `rustty_global::GlobalState<Action>`, NOT core's `Action=()` STATE);
  `rustty_apprt::init` stays cli-free (no `apprt→cli` cycle). Dispatch: action →
  `exit(action.run())`; `app_runtime == .none` + no action → usage banner + exit 0;
  the GUI else-branch (App.create + apprt.App.init/run over xev) is the Phase-3
  `unimplemented!()` leaf. `main_c.zig`(3t)→apprt-c + `lib_vt.zig`(1t)→vt-c re-homed
  in the test-count crate_map (bin does NOT dep apprt-c); main_gen/main_wasm/
  main_build_data (0t gen/wasm artifacts) deferred.
- **The "it runs" KAT (`crates/rustty/tests/cli_smoke.rs`, 9 tests).** Spawns the
  BUILT `env!("CARGO_BIN_EXE_rustty")` and asserts end-to-end: `+version` (exit 0,
  stdout starts `rustty ` — NOT the renderer/libxev stub content, ADV-FIX 1) /
  `--version` / `+list-actions` (non-empty) / `+validate-config` (exit 0 valid,
  exit 1 + diagnostic broken) / `+ssh-cache --add` then list shows the entry /
  `+new-window` (exit 1 + "not supported on this platform" — the faithful `none`
  IPC seam, ADV-FIX 4) / no-args (exit 0 + usage banner, `.none`-scoped) / `+bogus`
  (exit 1 + "unknown CLI action"). **The headless product actually executes.**
- **Gates:** workspace build clean + bin tests (2 unit + 9 KAT) + apprt-c
  (3 re-homed main_c + smoke) + adjacent regressions (vt 2019 / core 15 /
  apprt-api 20 / termio 44) + clippy `-D warnings` + fmt + acyclic (bin a clean top
  sink over cli+apprt; no `apprt→cli` edge) + test-counts (main 0/1, main_ghostty
  0/1, main_c 3/3, lib_vt 0/308 GREEN) + scoped-G3b (main.zig + main_ghostty.zig
  distinctive, missing=0) all GREEN.
- **Headless framing (honest):** `rustty +version`/`+list-fonts`/`+validate-config`/
  `+show-config`/`+list-actions`/`+ssh-cache`/`+ssh …` all RUN for real; `rustty`
  with no action prints the usage banner and exits (it is the headless helper CLI,
  not a window). **GUI launch = Phase-3** (needs the live embedded/GTK apprt backend
  + xev D6). DEFERRALS.md "rustty bin C6" tracks the GUI leaf + the log-facade /
  help_strings / vaxis / version-stub-line continuations.

## rustty-xev (D6 libxev port) — Milestone A ✅ DONE (2026-07-03) — isolated real `rustty-xev`

The libxev event-loop LIBRARY (mitchellh/libxev, ghostty's pinned commit) ported
faithfully into `crates/rustty-xev`, **macOS/kqueue only** (XEV_BLUEPRINT §0: on
macOS `xev.Dynamic` IS the static kqueue `Api()`, so `dynamic.zig` is comptime-
elided/deferred, not weakened). Chunks C1–C7 landed gate-green; the placeholder
handle types were filled IN PLACE (downstream `use` paths never changed —
termio/core `pub use rustty_xev::*` / `use rustty_xev as xev`).

- **What's real:** `queue`/`queue_mpsc`/`heap` (raw-ptr intrusive FIFO/MPSC/pairing-
  heap, C1, Miri-clean) → `darwin` mach bindings (C2) → the `Loop` core
  (`Completion`/`Operation`/`Result` + the exact `tick` ordering, C3) → `Async`
  mach-port wakeup (C4) → `Process` EVFILT_PROC wait (C5) → `Stream`/`WriteQueue`/
  `WriteRequest` + the `queueWrite` ordered-write engine (C6) → **C7 the finale:**
  the `lib.rs` package surface = kqueue `Api()` (`main.zig` re-exports + `api.zig`'s
  2 tests + `backend.zig`'s `Backend` enum, `default() == Kqueue`), `Timer::reset`/
  `cancel` un-deferred.
- **Tests:** **36 R1-parity `#[test]`** (queue 1 + mpsc 1 + heap 6 + kqueue 12 +
  async 3 + timer 5 + process 3 + stream 4 + api 1) + **4 live KATs** (async/timer/
  process/pty round-trips on real kqueue) + 2 reify/sanity margins = **42 total**,
  `cargo test -p rustty-xev` green. `check_test_counts.py` maps every in-scope module
  ([ADV-FIX F7], all rust ≥ zig). The one honest skip-stub is `kqueue: file IO on
  thread pool` (ThreadPool body deferred — ghostty passes no pool).
- **Discipline:** raw pointers end-to-end (loop holds `*mut Completion` into caller
  memory; callbacks get `*mut Loop`/`*mut Completion` — no `&mut`/lifetime aliasing,
  Stacked-Borrows-safe); `unsafe impl Send` with single-thread-affinity notes
  ([ADV-FIX F2]); hand-written `Default` for `Completion`/`WriteQueue`/`WriteRequest`
  + `Async: Copy` + `NotifyError: Debug+Clone+Copy+PartialEq+Eq` ([ADV-FIX F3], the
  load-bearing termio/core trait surface). `check_acyclic.sh` green (rustty-xev deps
  only `rustty-global` + `libc`; NO edge to termio/core/os). Miri covers C1; the
  kqueue/mach syscall chunks (C3–C6) lean on the live KATs (MIRI_EXCEPTIONS.md).
- **Deferred (DEFERRALS.md "rustty-xev C7"):** `dynamic.zig` / ThreadPool body /
  `watcher/file`|`tcp`|`udp` / other backends (epoll/io_uring/iocp/wasi_poll) /
  `c_api.zig` + the one skip-stub. [ADV-FIX F6] flatpak/vt-search xev refs resolve
  LATER via direct `os → xev` / `vt → xev` edges (NOT a `rustty_global::xev` re-
  export, which would cycle).

**Milestone B (NEXT):** un-defer **TERMIO ONLY** ([ADV-FIX F8]) — `Thread.run` loop +
`Termio.init`/`threadEnter`/`processOutput` + `Exec` `Subprocess.start`/`ReadThread`/
`queueWrite` + the core `Async::notify()` leaves termio's threads poke — capped with a
**live-shell KAT** (spawn a shell in a pty, pump the loop, assert output round-trips).
`renderer.Thread` stays the **Phase-3 GPU tier** (GPU-*and*-xev-blocked; landing
rustty-xev does not unblock it — listing it would false-green).

## INTEGRATION TIER — the critical path to the libghostty embedding milestone (original notes)
Dependency-ordered, apprt-api now unblocks it:
**quirks** (57 loc, tiny leaf, imports font — needs a crate-placement decision) + **Command** (911 loc/10t, now unblocked by apprt-api — placement decision) + **crash** (sentry-sys ready) → **termio** (pty/exec/thread, 30 tests — the "run a real shell" layer; pty.rs already ported in rustty-os) → **core** (Surface.zig/App.zig — the big one; fills apprt-api's growth-point trait methods + the mailbox/queue seam) → **apprt/embedded** (2250 loc, 71 C-ABI exports — THE libghostty embedding milestone, via vt-c cbindgen+pinned-padding+const-size-assert) + **none.zig** (headless). Then **cli** + **bin**. Crown-jewel-adjacent — needs design + careful review (not background dispatch).

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
