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

## Config cluster — C1 ✅, C2 ✅ DONE (2026-06-26), C3 next
C1 leaf value-types (36t) + C2 the giant Config.zig (10934loc, 113 tests, 203 fields
== 203 Key variants — field-parity audit caught 7 dropped fields incl font-size; the
4 comptime passes via Target-trait + exhaustiveness test; load/finalize/parseManuallyHook/
Replay/theme machinery). 169 config tests, count-gate GREEN, no regression. Deferred:
RepeatableCodepointMap (font/CodepointMap.zig unported), FileFormatter docs (help_strings),
url regex (oniguruma-sys). NEXT: C3 = CApi.zig(8t) + c_get.zig(6t) in rustty-config.

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
