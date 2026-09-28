# Parity audit — pkg

167 oracle units were fully read across 9 shards (pkg/breakpad, pkg/fontconfig, pkg/dcimgui, pkg/freetype, pkg/glslang, pkg/gtk4-layer-shell, pkg/harfbuzz, pkg/highway, pkg/libintl, pkg/libpng, pkg/libxml2, pkg/macos + graphics/iosurface/os/text/video, pkg/oniguruma, pkg/opengl, pkg/sentry, pkg/simdutf, pkg/spirv-cross, pkg/wuffs, pkg/zlib, plus src/App.zig and src/Command.zig carried in the pkg/zlib shard) — 14,431 lines read, 100% of the lines in scope, with **no units left partially read**.
46 findings were raised: **21 CONFIRMED, 12 DISPUTED, 13 REFUTED, 0 UNVERIFIED**. By corrected severity over the 33 surviving (confirmed + disputed) findings: **4 high, 7 medium, 22 low**.
Coverage status split across the 167 units: 63 ported, 10 partial, 5 missing, 74 replaced (third-party binding/build layers legitimately re-expressed as `crates-sys` / objc2 / crates.io crates), 15 not-applicable.
The single most important gap is **S34-F1**: `crates-sys/highway-sys/cpp/detect.c` enables the AVX2 Highway target from CPUID bits alone, dropping the oracle's `xgetbv`/XCR0 OS-support gate and its LZCNT/BMI/BMI2 requirements — on a host that advertises AVX2 without OS-saved AVX state (or without BMI2), rustty can dispatch AVX2 kernels the oracle refuses, up to a SIGILL. The whole non-x86/non-aarch64 half of Highway's runtime detection (PowerPC, s390x, RISC-V, LoongArch) is also absent, and aarch64 SVE bits are probed and discarded.

## Coverage

### Shard 31 — pkg/breakpad, pkg/dcimgui, pkg/fontconfig (1,555 lines, 19 units)

| Oracle unit                   | Rustty counterpart(s)                                                                            | Status   | Note                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| ----------------------------- | ------------------------------------------------------------------------------------------------ | -------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| pkg/breakpad/build.zig        | crates-sys/breakpad-sys/build.rs; src/lib.rs; vendor/src/third_party/lss/linux_syscall_support.h | replaced | Zig build plumbing -> cc-crate build.rs. All 7 source lists are verbatim (build.rs:37-111), per-target selection identical (build.rs:149-165), link_libcpp => cpp(true), empty flags list preserved, `src` include + lss vendor header byte-identical, installHeadersDirectory(src) => cargo:include consumed by sentry-sys (DEP_BREAKPAD_INCLUDE). apple_sdk.addPaths replaced by the native cc toolchain SDK. KAT test asserts archive contains client objects. |
| pkg/dcimgui/build.zig         | crates-sys/dcimgui-sys/build.rs; ext.cpp; Cargo.toml                                             | replaced | cc+bindgen build.rs. Options fixed to the values SharedDeps.zig:597-606 passes; backend-osx is NOT mirrored (see S31-F3). Common flags all present (build.rs:79-127). Five core TUs + imgui_freetype.cpp + dcimgui.cpp/dcimgui_internal.cpp + ext.cpp compiled; imgui_impl_metal.mm on macOS, imgui_impl_opengl3.cpp elsewhere. ext.cpp equals the oracle plus a rustty-only metal include block (no observable effect).                                          |
| pkg/dcimgui/main.zig          | crates-sys/dcimgui-sys/src/lib.rs; wrapper.h                                                     | partial  | bindgen over wrapper.h with the same defines. OpenGL3/Metal/ext/DockBuilder-subset/DockSpace symbols present. MISSING (all unused by ghostty src/): ImGui_ImplOSX_* (3), 10 further DockBuilder externs, 12 further ImGuiDockNodeFlagsPrivate constants — S31-F3/F4/F5.                                                                                                                                                                                           |
| pkg/fontconfig/build.zig      | crates-sys/fontconfig-sys/build.rs; Cargo.toml; vendor/override/include                          | replaced | 28 srcs verbatim; common flags verbatim; SIZEOF/ALIGNOF by pointer width; Windows arm verbatim; off-Windows HAVE_* + FC_CACHEDIR/FC_DEFAULT_FONTS verbatim; libxml2 defines always on; override headers byte-identical. GAPS: FreeBSD dir arm and Windows+libxml2 -Werror flag — S31-F1/F2.                                                                                                                                                                       |
| pkg/fontconfig/c.zig          | crates-sys/fontconfig-sys/wrapper.h; src/lib.rs                                                  | replaced | @cImport(fontconfig/fontconfig.h) => bindgen over a wrapper.h including exactly that header.                                                                                                                                                                                                                                                                                                                                                                      |
| pkg/fontconfig/char_set.zig   | crates/rustty-font-shaper/src/fc.rs:901-935                                                      | ported   | CharSet create/destroy/add_char/has_char; test "create" => char_set_create with the same 3 assertions.                                                                                                                                                                                                                                                                                                                                                            |
| pkg/fontconfig/common.zig     | crates/rustty-font-shaper/src/fc.rs:139-355                                                      | ported   | Weight/Slant/Spacing repr(u32) with FC_* discriminants; Property 50 variants + cval table (every string checked); Result + to_error mapping identical; MatchKind. test "cval" ported.                                                                                                                                                                                                                                                                             |
| pkg/fontconfig/config.zig     | crates/rustty-font-shaper/src/fc.rs:571-656                                                      | ported   | Config destroy/font_list/font_sort/font_render_prepare/substitute_with_pat, FontSortResult.                                                                                                                                                                                                                                                                                                                                                                       |
| pkg/fontconfig/error.zig      | crates/rustty-font-shaper/src/fc.rs:45-53                                                        | ported   | Error enum with the same 5 atoms.                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| pkg/fontconfig/font_set.zig   | crates/rustty-font-shaper/src/fc.rs:993-1039                                                     | ported   | FontSet create/destroy/fonts(null=>empty)/add/print; test "create" ported.                                                                                                                                                                                                                                                                                                                                                                                        |
| pkg/fontconfig/init.zig       | crates/rustty-font-shaper/src/fc.rs:60-126                                                       | ported   | init/fini/init_load_config/init_load_config_and_fonts/version; 4 tests ported with the same assertions (test-only RwLock added to serialise FcFini).                                                                                                                                                                                                                                                                                                              |
| pkg/fontconfig/lang_set.zig   | crates/rustty-font-shaper/src/fc.rs:941-986                                                      | ported   | LangSet create/destroy/add_lang/has_lang; both tests ported identically.                                                                                                                                                                                                                                                                                                                                                                                          |
| pkg/fontconfig/main.zig       | crates/rustty-font-shaper/src/fc.rs; src/lib.rs:64                                               | ported   | All re-exports live in the single fc module (cfg-gated off macOS/Windows as the reference is). refAllDecls => ref_all_decls; `_ = @import("test.zig")` => fc_list/fc_match.                                                                                                                                                                                                                                                                                       |
| pkg/fontconfig/matrix.zig     | crates/rustty-font-shaper/src/fc.rs:534-541                                                      | ported   | repr(C) Matrix {xx,xy,yx,yy: f64}.                                                                                                                                                                                                                                                                                                                                                                                                                                |
| pkg/fontconfig/object_set.zig | crates/rustty-font-shaper/src/fc.rs:567-597                                                      | ported   | ObjectSet create/destroy/add(Property); test "create" ported.                                                                                                                                                                                                                                                                                                                                                                                                     |
| pkg/fontconfig/pattern.zig    | crates/rustty-font-shaper/src/fc.rs:663-897                                                      | ported   | Pattern full surface, ObjectIterator.next and ValueIterator.next with identical short-circuits; both tests ported with equal assertions.                                                                                                                                                                                                                                                                                                                          |
| pkg/fontconfig/range.zig      | crates/rustty-font-shaper/src/fc.rs:547-561                                                      | ported   | Range destroy.                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| pkg/fontconfig/test.zig       | crates/rustty-font-shaper/src/fc.rs:1147-1265                                                    | ported   | fc_list and fc_match with the same assertion counts; the reference's per-font destroy defer is deliberately not reproduced (S31-F6, refuted).                                                                                                                                                                                                                                                                                                                     |
| pkg/fontconfig/value.zig      | crates/rustty-font-shaper/src/fc.rs:362-527                                                      | ported   | Type repr(i32) 11 tags, Value 11-variant enum + tag()/init()/cval() with the same per-tag mapping, ValueBinding.                                                                                                                                                                                                                                                                                                                                                  |

### Shard 32 — pkg/freetype, pkg/glslang (1,361 lines, 16 units)

| Oracle unit                   | Rustty counterpart(s)                                                                      | Status   | Note                                                                                                                                                                                                                               |
| ----------------------------- | ------------------------------------------------------------------------------------------ | -------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| pkg/freetype/Library.zig      | crates/rustty-font-shaper/src/ft.rs:623-716                                                | ported   | Library{handle}, init/deinit/version/initFace/initMemoryFace/doneMMVar, Version + toString ported 1:1; test "basics" ported with the same assertions plus an additive exact-format check.                                          |
| pkg/freetype/build.zig        | crates-sys/freetype-sys/build.rs; Cargo.toml                                               | replaced | Verbatim 41-entry srcs list, same flags, same per-OS ftsystem.c/ftdebug.c selection, zlib via zlib-sys, libpng compiled in. Build-only differences: no system-integration option, no Windows ftver.rc, vendored warnings silenced. |
| pkg/freetype/c.zig            | crates-sys/freetype-sys/wrapper.h; src/lib.rs                                              | replaced | @cImport of freetype-zig.h -> bindgen over a wrapper.h including the identical 9 headers.                                                                                                                                          |
| pkg/freetype/computations.zig | crates/rustty-font-shaper/src/ft.rs:318-322                                                | ported   | mul_fix -> FT_MulFix; only difference is @intCast vs `as` truncation, not observable in ReleaseFast.                                                                                                                               |
| pkg/freetype/errors.zig       | crates/rustty-font-shaper/src/ft.rs:26-311                                                 | ported   | All 90 variants; int_to_error covers every FT_Err_* arm in the same order; error_to_int mirrors errorToInt. Test ported with both original assertions plus additive round-trips.                                                   |
| pkg/freetype/face.zig         | crates/rustty-font-shaper/src/ft.rs:378-957,1044-1329                                      | ported   | Full Face surface incl. getSfntTable comptime dispatch, loadSfntTable len-probe, LoadFlags packed layout and Target enum; all 4 tests ported.                                                                                      |
| pkg/freetype/main.zig         | crates/rustty-font-shaper/src/ft.rs; src/lib.rs:53                                         | ported   | Re-exports c/testing/Library/Error/Face/LoadFlags/Tag/mul_fix; refAllDecls ported.                                                                                                                                                 |
| pkg/freetype/tag.zig          | crates/rustty-font-shaper/src/ft.rs:324-360                                                | ported   | Tag{d,c,b,a}, init/str/to_u32 reproducing the packed-struct @bitCast.                                                                                                                                                              |
| pkg/freetype/test.zig         | crates/rustty-font-shaper/src/ft.rs:959-966; res/FiraCode-Regular.ttf                      | ported   | FONT_REGULAR fixture byte-identical to the oracle's (cmp verified).                                                                                                                                                                |
| pkg/glslang/build.zig         | crates-sys/glslang-sys/build.rs; Cargo.toml                                                | replaced | Verbatim 40-file list, Unix/Windows ossource.cpp selection, override/build_info.h byte-identical, same sanitizer/std/PIC flags. Build-only extra: `-include cstdint`.                                                              |
| pkg/glslang/c.zig             | crates-sys/glslang-sys/wrapper.h; src/lib.rs                                               | replaced | bindgen over the same two headers; prepend_enum_name(false) so GLSLANG_* constants surface unadorned.                                                                                                                              |
| pkg/glslang/init.zig          | crates/rustty-renderer/src/shadertoy.rs:717-732                                            | partial  | init() ported; finalize() has no wrapper (never called in the oracle; raw FFI available) — S32-F2 (refuted).                                                                                                                       |
| pkg/glslang/main.zig          | crates/rustty-renderer/src/shadertoy.rs; crates-sys/glslang-sys/src/lib.rs                 | replaced | No standalone wrapper crate; the pkg surface is raw glslang-sys FFI + wrapper methods inlined into shadertoy.rs.                                                                                                                   |
| pkg/glslang/program.zig       | crates/rustty-renderer/src/shadertoy.rs:392-499; crates-sys/glslang-sys/src/lib.rs:101-129 | partial  | Every method the oracle uses is inlined with identical semantics; the anonymous create/delete test is covered by the glslang-sys KAT. spirvGet/spirvGetMessages unwrapped (unused).                                                |
| pkg/glslang/shader.zig        | crates/rustty-renderer/src/shadertoy.rs:362-481; crates-sys/glslang-sys/src/lib.rs:36-133  | partial  | create/delete/preprocess/parse/getInfoLog/getDebugInfoLog inlined faithfully; the simple.frag fixture is not vendored — S32-F1 (refuted: shadertoy_prefix.glsl exercises the same shape).                                          |
| pkg/glslang/test.zig          | crates/rustty-renderer/src/shadertoy.rs:735-753                                            | ported   | ensureInit ported with the same never-stored `initialized` flag, so init runs on every call exactly like the oracle.                                                                                                               |

### Shard 33 — pkg/gtk4-layer-shell, pkg/harfbuzz (1,617 lines, 14 units)

| Oracle unit                       | Rustty counterpart(s)                                                         | Status   | Note                                                                                                                                                                                          |
| --------------------------------- | ----------------------------------------------------------------------------- | -------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| pkg/gtk4-layer-shell/build.zig    | crates/rustty-apprt/Cargo.toml:102,372-376; xtask/docker/debian/Dockerfile:44 | replaced | Replaced by the crates.io `gtk4-layer-shell = 0.8` binding linking the system libgtk4-layer-shell-0 via pkg-config; the reference's system-vs-in-tree switch collapses to system-only.        |
| pkg/gtk4-layer-shell/src/main.zig | crates/rustty-apprt/src/gtk/winproto/wayland.rs:84,319-334,410-436,887-965    | replaced | All enums and all nine use sites present with identical branching, values and warnings. One low-fidelity use-site difference: set_namespace goes through from_utf8_lossy — S33-F1.            |
| pkg/harfbuzz/blob.zig             | crates/rustty-font-shaper/src/hb.rs:554-681,1554-1580                         | ported   | MemoryMode, Blob create/destroy/set_user_data/get_user_data (transmuted destroy cb, same ABI); test ported with both assertions plus additive ones.                                           |
| pkg/harfbuzz/buffer.zig           | crates/rustty-font-shaper/src/hb.rs:112-450,1607-1643                         | ported   | Full Buffer surface, ContentType, ClusterLevel, GlyphInfo/GlyphPosition bindgen aliases; test "create" ported.                                                                                |
| pkg/harfbuzz/build.zig            | crates-sys/harfbuzz-sys/build.rs; wrapper.h; Cargo.toml                       | replaced | Vendored HarfBuzz 11.0.0 built with the same -D flags; bindgen over hb.h + hb-ft.h (+ hb-coretext.h on Apple); HAVE_CORETEXT=1 on macOS. Build-only differences; doc comments stale — S33-F2. |
| pkg/harfbuzz/common.zig           | crates/rustty-font-shaper/src/hb.rs:192-225,704-1372,1582-1605                | ported   | Direction, all 164 Script variants in declaration order with to_c/from_c, Language, Feature; test ported plus distinctness/round-trip assertions.                                             |
| pkg/harfbuzz/coretext.zig         | crates/rustty-font-shaper/src/hb.rs:65-80,1676-1703                           | ported   | create_font_coretext (cfg macos) with null->HarfbuzzFailed; test ported (macOS-only, same Monaco/12pt sequence).                                                                              |
| pkg/harfbuzz/errors.zig           | crates/rustty-font-shaper/src/hb.rs:23-26                                     | ported   | Error.HarfbuzzFailed -> unit struct used as the Err type everywhere.                                                                                                                          |
| pkg/harfbuzz/face.zig             | crates/rustty-font-shaper/src/hb.rs:683-702                                   | ported   | Face { handle } + destroy.                                                                                                                                                                    |
| pkg/harfbuzz/font.zig             | crates/rustty-font-shaper/src/hb.rs:28-63                                     | ported   | Font create/destroy/set_scale (u32->i32 via `as`; differs only for illegal-in-reference values).                                                                                              |
| pkg/harfbuzz/freetype.zig         | crates/rustty-font-shaper/src/hb.rs:473-525,1645-1674                         | ported   | create_face/create_font via hb_ft_*_create_referenced, set_font_funcs; test ported with the same sequence.                                                                                    |
| pkg/harfbuzz/main.zig             | crates/rustty-font-shaper/src/hb.rs:1-21,1705-1789                            | ported   | All re-exports are top-level items of hb; `harfbuzz.c` is the harfbuzz_sys crate; refAllDecls ported.                                                                                         |
| pkg/harfbuzz/shape.zig            | crates/rustty-font-shaper/src/hb.rs:452-475                                   | ported   | shape() with identical null-pointer-when-empty and len-when-Some semantics.                                                                                                                   |
| pkg/harfbuzz/version.zig          | crates/rustty-font-shaper/src/hb.rs:527-566,1532-1552                         | ported   | Version, version(), version_at_least, version_string; test ported with both assertions plus an additive one.                                                                                  |

### Shard 34 — pkg/highway, pkg/libintl, pkg/libpng, pkg/libxml2 (1,142 lines, 15 units)

| Oracle unit                               | Rustty counterpart(s)                            | Status   | Note                                                                                                                                                                                                                                                                                             |
| ----------------------------------------- | ------------------------------------------------ | -------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| pkg/highway/build.zig                     | crates-sys/highway-sys/build.rs; Cargo.toml      | replaced | Same three TUs (abort.cc/per_target.cc byte-identical, targets.cpp identical modulo ghostty_->rustty_ rename), same behaviour-affecting flags, header dir exported via cargo:include, test step -> #[cfg(test)]. Diagnostic -W* flags replaced by -w. MSVC-only flags absent — S34-F9 (refuted). |
| pkg/highway/src/detect.zig                | crates-sys/highway-sys/cpp/detect.c              | partial  | Arch dispatch ported as an #if ladder: aarch64 (darwin/linux/other) and x86 present; ppc/s390x/riscv/loongarch arms collapse to `return 0` (detect.c:188-190).                                                                                                                                   |
| pkg/highway/src/main.zig                  | crates-sys/highway-sys/src/lib.rs                | partial  | hwy_supported_targets extern + test present (assertion stronger than the oracle's). The typed `Targets` bitcast wrapper has no Rust counterpart — S34-F8 (refuted).                                                                                                                              |
| pkg/highway/src/targets.zig               | crates-sys/highway-sys/cpp/detect.c:19-29        | partial  | Only 8 of 29 comptime-asserted bit positions reproduced as #defines; those 8 match the oracle exactly.                                                                                                                                                                                           |
| pkg/highway/src/detect/aarch64_darwin.zig | crates-sys/highway-sys/cpp/detect.c:33-62        | ported   | neon set unconditionally, FEAT_BF16 probed via sysctlbyname with identical semantics, no SVE. Identical.                                                                                                                                                                                         |
| pkg/highway/src/detect/aarch64_linux.zig  | crates-sys/highway-sys/cpp/detect.c:64-118       | partial  | HWCAP/HWCAP2 constants and neon/neon_bf16 logic match; sve_vector_bytes ported but its result is discarded and no SVE bit is ever set — S34-F3.                                                                                                                                                  |
| pkg/highway/src/detect/linux.zig          | crates-sys/highway-sys/cpp/detect.c:65-90        | replaced | std.os.linux syscall shims replaced by libc getauxval()/prctl(); observably equivalent.                                                                                                                                                                                                          |
| pkg/highway/src/detect/loongarch.zig      | —                                                | missing  | No loongarch branch; falls to `return 0` — S34-F7 (disputed: unreachable in Highway's dispatch gate).                                                                                                                                                                                            |
| pkg/highway/src/detect/ppc.zig            | —                                                | missing  | No powerpc branch; falls to `return 0` — S34-F4.                                                                                                                                                                                                                                                 |
| pkg/highway/src/detect/riscv.zig          | —                                                | missing  | No riscv branch; falls to `return 0` — S34-F6.                                                                                                                                                                                                                                                   |
| pkg/highway/src/detect/s390x.zig          | —                                                | missing  | No s390x branch; falls to `return 0` — S34-F5.                                                                                                                                                                                                                                                   |
| pkg/highway/src/detect/x86.zig            | crates-sys/highway-sys/cpp/detect.c:127-186      | partial  | cpuid guard, SSE2/SSSE3/SSE4 identical. AVX2 gate differs (no xgetbv, no LZCNT/BMI/BMI2) — S34-F1; AVX-512/AVX10/isAMD entirely absent — S34-F2.                                                                                                                                                 |
| pkg/libintl/build.zig                     | crates-sys/gettext-sys/build.rs; src/lib.rs      | replaced | Same 30 sources in the same order, same flags, same include layering, macOS-only; vendor/config.h + headers diff-identical. Extra CoreFoundation link directive is additive.                                                                                                                     |
| pkg/libpng/build.zig                      | crates-sys/libpng-sys/build.rs; Cargo.toml       | replaced | Same 15 png*.c TUs in order, same -DPNG_*_OPT=0 flags, MSVC ubsan opt-outs gated on target_env, Linux -lm, ghostty pnglibconf.h vendored diff-identical. zlib via zlib-sys.                                                                                                                      |
| pkg/libxml2/build.zig                     | crates-sys/libxml2-sys/build.rs; vendor/override | replaced | Same 44 srcs in order, same include order, identical version defines incl. the oracle's 201105 quirk, same HAVE_*/Options defaults; override/ dir diff -r identical.                                                                                                                             |

### Shard 35 — pkg/macos (foundation, dispatch, animation, carbon) (1,254 lines, 27 units)

| Oracle unit                                | Rustty counterpart(s)                                                                                                                                                                         | Status         | Note                                                                                                                                                                                                                         |
| ------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| pkg/macos/animation.zig                    | crates/rustty-renderer/src/metal/iosurface_layer.rs:20,147                                                                                                                                    | replaced       | kCAGravity* externs -> objc2_quartz_core; the only oracle use site (contentsGravity=TopLeft) is present verbatim.                                                                                                            |
| pkg/macos/build.zig                        | crates/rustty-os/build.rs:38-47; cbits/log_shim.c; keymap_darwin.rs:373,408; discovery.rs:921; shaper/coretext.rs:96; face/coretext.rs:73; cvdisplaylink.rs:71; 3 Cargo.toml framework blocks | replaced       | translate-c + static lib + framework linking replaced by objc2-* crates and per-site `#[link(kind="framework")]`. os/zig_macos.c -> cbits/log_shim.c. text/ext.c has no counterpart but its only consumer is a wrapper test. |
| pkg/macos/carbon.zig                       | crates/rustty-input/src/keymap_darwin.rs:373-400                                                                                                                                              | replaced       | Pure re-export; the only Carbon consumer declares its own extern block.                                                                                                                                                      |
| pkg/macos/dispatch.zig                     | crates/rustty-renderer/src/metal/iosurface_layer.rs:197-200; shaders.rs:429-432                                                                                                               | replaced       | dispatch_async/sync -> dispatch2 crate; the one oracle use site is present.                                                                                                                                                  |
| pkg/macos/foundation.zig                   | discovery.rs:860-870; shaper/coretext.rs:60-66; face/coretext.rs:39; keymap_darwin.rs:403-450; rustty-os/src/locale.rs:346-360                                                                | replaced       | Module index covered by objc2-core-foundation plus hand-declared CF externs.                                                                                                                                                 |
| pkg/macos/graphics.zig                     | rustty-font-shaper/Cargo.toml:123; face/coretext.rs:73; metal/metal_impl.rs:10                                                                                                                | replaced       | Module index only; re-exports -> objc2-core-graphics features.                                                                                                                                                               |
| pkg/macos/iosurface.zig                    | metal/target.rs:5; rustty-apprt-c/Cargo.toml:176                                                                                                                                              | replaced       | Module index only -> objc2-io-surface.                                                                                                                                                                                       |
| pkg/macos/main.zig                         | —                                                                                                                                                                                             | not-applicable | Zig package root; Rust has no single 'macos' crate. refAllDecls is a compile-smoke idiom with no assertions.                                                                                                                 |
| pkg/macos/os.zig                           | crates/rustty-os/src/macos.rs:219,358                                                                                                                                                         | replaced       | Module index; Log/LogType and signpost ported in rustty-os (bodies in shard 36).                                                                                                                                             |
| pkg/macos/text.zig                         | rustty-font-shaper/Cargo.toml:128; discovery.rs:864-870; shaper/coretext.rs:60-100                                                                                                            | replaced       | Module index -> objc2-core-text.                                                                                                                                                                                             |
| pkg/macos/video.zig                        | crates/rustty-renderer/src/metal/cvdisplaylink.rs:1-99                                                                                                                                        | replaced       | Module index; DisplayLink ported faithfully; PixelFormat body in shard 37.                                                                                                                                                   |
| pkg/macos/animation/c.zig                  | —                                                                                                                                                                                             | not-applicable | One-line re-export of the translate-c module.                                                                                                                                                                                |
| pkg/macos/carbon/c.zig                     | —                                                                                                                                                                                             | not-applicable | One-line re-export of the translate-c module.                                                                                                                                                                                |
| pkg/macos/dispatch/c.zig                   | —                                                                                                                                                                                             | not-applicable | One-line re-export of the translate-c module.                                                                                                                                                                                |
| pkg/macos/dispatch/data.zig                | crates/rustty-renderer/src/metal/shaders.rs:429-435                                                                                                                                           | replaced       | Data.create -> dispatch2::DispatchData::from_bytes (DESTRUCTOR_DEFAULT copies the buffer, so the queue difference is unobservable). OOM-on-NULL becomes an unwrap on a never-hit path.                                       |
| pkg/macos/dispatch/queue.zig               | crates/rustty-renderer/src/metal/iosurface_layer.rs:200                                                                                                                                       | replaced       | getMain -> dispatch2::DispatchQueue::main(); both oracle use sites covered.                                                                                                                                                  |
| pkg/macos/foundation/array.zig             | discovery.rs:864; shaper/coretext.rs:63; locale.rs:356-360                                                                                                                                    | replaced       | Array/MutableArray use sites covered by CFArray + NSLocale::preferredLanguages. createCopy/removeValue/sortValues have no oracle src use site. Wrapper tests have no counterpart — S35-F1.                                   |
| pkg/macos/foundation/attributed_string.zig | shaper/coretext.rs:641-670; Cargo.toml:122                                                                                                                                                    | replaced       | AttributedString.create covered; MutableAttributedString used only by wrapper tests.                                                                                                                                         |
| pkg/macos/foundation/base.zig              | metal/buffer.rs:32,189,255; discovery.rs:865                                                                                                                                                  | replaced       | Range.init -> NSRange::new / CFRange. ComparisonResult only used by sortValues (no use site).                                                                                                                                |
| pkg/macos/foundation/c.zig                 | —                                                                                                                                                                                             | not-applicable | One-line re-export.                                                                                                                                                                                                          |
| pkg/macos/foundation/character_set.zig     | discovery.rs:1072-1088                                                                                                                                                                        | replaced       | createWithCharactersInRange covered with kCTFontCharacterSetAttribute; createWithCharactersInString has no use site.                                                                                                         |
| pkg/macos/foundation/data.zig              | face/coretext.rs:204,1307-1318; keymap_darwin.rs:417,446-448                                                                                                                                  | replaced       | createWithBytesNoCopy (kCFAllocatorNull) -> new_data_no_copy with OOM-on-NULL; getPointer covered.                                                                                                                           |
| pkg/macos/foundation/dictionary.zig        | shaper/coretext.rs:1286-1305; discovery.rs:968,985-992,1720                                                                                                                                   | replaced       | Dictionary/MutableDictionary create + getValue covered; getKeysAndValues/createMutableCopy have no use site.                                                                                                                 |
| pkg/macos/foundation/number.zig            | discovery.rs:995-1005,1098-1140,1720; face/coretext.rs:436-438; shaper/coretext.rs:289-291,404-406; metal/target.rs:245-247                                                                   | replaced       | Number.create/getValue -> CFNumber with the matching CFNumberType at every use site.                                                                                                                                         |
| pkg/macos/foundation/string.zig            | deferred_face.rs:192-216; face/coretext.rs:89-104; shaper/coretext.rs:641-661,1269-1284; discovery.rs:1025-1040; keymap_darwin.rs:420-442                                                     | replaced       | createWithBytes / createWithCharactersNoCopy / cstring all covered; the CF_INLINE surrogate-pair helper reproduced verbatim 4x. hasPrefix/compare/MutableString have no use site.                                            |
| pkg/macos/foundation/type.zig              | keymap_darwin.rs:410; rustty-os/src/cf_release_thread.rs                                                                                                                                      | replaced       | CFRelease/CFRetain -> CFRetained RAII, with the raw extern kept at the two oracle use sites.                                                                                                                                 |
| pkg/macos/foundation/url.zig               | deferred_face.rs:455-480                                                                                                                                                                      | replaced       | copyPath + createStringByReplacingPercentEscapes used only by DeferredFace.loadCoreTextFreetype, carried as a commented-out block under the locked macOS-first fork.                                                         |

### Shard 36 — pkg/macos/graphics, /iosurface, /os, /text (2,098 lines, 26 units)

| Oracle unit                             | Rustty counterpart(s)                                                                                       | Status         | Note                                                                                                                                                                                          |
| --------------------------------------- | ----------------------------------------------------------------------------------------------------------- | -------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| pkg/macos/graphics/affine_transform.zig | face/coretext.rs:40,138                                                                                     | replaced       | objc2_core_foundation::CGAffineTransform (same layout); the only oracle use site (italic_skew) present.                                                                                       |
| pkg/macos/graphics/bitmap_context.zig   | face/coretext.rs:70-85,689-700                                                                              | replaced       | objc2 omits CGBitmapContextCreate, so rustty declares the raw symbol and maps NULL -> OutOfMemory exactly like the wrapper. Wrapper smoke test has no counterpart — S36-F6 (refuted).         |
| pkg/macos/graphics/c.zig                | —                                                                                                           | not-applicable | One-line re-export superseded by objc2-core-graphics.                                                                                                                                         |
| pkg/macos/graphics/color_space.zig      | face/coretext.rs:657,665; metal/target.rs:80-98                                                             | partial        | createNamed -> CGColorSpace::with_name for the glyph rasterizer; the Target.zig displayP3 use is SKIPPED — S36-F1.                                                                            |
| pkg/macos/graphics/context.zig          | face/coretext.rs:703-777                                                                                    | replaced       | Every setter the reference uses maps 1:1 in the same order with the same arguments.                                                                                                           |
| pkg/macos/graphics/font.zig             | face/coretext.rs:47                                                                                         | replaced       | Glyph = CGGlyph (u16).                                                                                                                                                                        |
| pkg/macos/graphics/geometry.zig         | face/coretext.rs:40                                                                                         | replaced       | Point/Rect/Size -> objc2-core-foundation CG types.                                                                                                                                            |
| pkg/macos/graphics/image.zig            | face/coretext.rs:660,668                                                                                    | replaced       | Alpha/byte-order constants OR'ed into the CGBitmapContextCreate opts as the reference does.                                                                                                   |
| pkg/macos/graphics/path.zig             | —                                                                                                           | not-applicable | Path/MutablePath used only inside pkg/macos wrapper tests; objc2-core-graphics CGPath covers the API.                                                                                         |
| pkg/macos/iosurface/c.zig               | —                                                                                                           | not-applicable | Re-export superseded by objc2-io-surface.                                                                                                                                                     |
| pkg/macos/iosurface/iosurface.zig       | metal/target.rs:80-198; iosurface_layer.rs:237                                                              | partial        | 4-key property dict + IOSurfaceCreate NULL->error rebuilt; accessors/lock/unlock map over. MISSING: colorspace/setColorSpace (S36-F1) and deinit's SetPurgeable (S36-F2, refuted).            |
| pkg/macos/os/c.zig                      | —                                                                                                           | not-applicable | Re-export; rustty-os binds os_log/os_signpost directly.                                                                                                                                       |
| pkg/macos/os/log.zig                    | rustty-os/src/macos.rs:219-343,759-775; cbits/log_shim.c; build.rs:37-47                                    | ported         | Log + LogType with the same OS_LOG_TYPE_* values; shim forwards "%{public}s" identically; test ported. Only nit: create does not check NULL — S36-F4.                                         |
| pkg/macos/os/signpost.zig               | rustty-os/src/macos.rs:357-757                                                                              | ported         | init (idempotent, dladdr), enabled, emit paths, Id/Type/Category constants all identical; tests ported as a superset.                                                                         |
| pkg/macos/text/c.zig                    | —                                                                                                           | not-applicable | Re-export superseded by objc2-core-text.                                                                                                                                                      |
| pkg/macos/text/font.zig                 | face/coretext.rs (7 ranges); discovery.rs:918-949; deferred_face.rs:235-260                                 | replaced       | objc2 CTFont; nullable-return wrappers re-bound raw where objc2 panics. FourCC/copyTable/metrics/draw all map over. Creation NULL->OutOfMemory paths lost to objc2 `expect` — S36-F3.         |
| pkg/macos/text/font_collection.zig      | discovery.rs:1225-1228,1520-1547                                                                            | replaced       | createWithFontDescriptors covered; the NULL->empty divergence reproduced in copy_matching_descriptors. Three wrapper tests have no counterpart; one of them pins rustty-owned logic — S36-F5. |
| pkg/macos/text/font_descriptor.zig      | discovery.rs (4 ranges); face/coretext.rs:264,428-456; deferred_face.rs:387; rustty-apprt-c/src/lib.rs:2893 | replaced       | CTFontDescriptor + the kCTFont* constants used directly; FontSymbolicTraits packed layout == objc2 bit constants. Four wrapper tests have no counterpart — S36-F6 (refuted).                  |
| pkg/macos/text/font_manager.zig         | face/coretext.rs:207-209                                                                                    | replaced       | createFontDescriptorFromData -> objc2 (None -> FontInitFailure as the use site does).                                                                                                         |
| pkg/macos/text/frame.zig                | —                                                                                                           | not-applicable | CTFrame used only in the framesetter wrapper test.                                                                                                                                            |
| pkg/macos/text/framesetter.zig          | —                                                                                                           | not-applicable | CTFramesetter unused in ghostty src.                                                                                                                                                          |
| pkg/macos/text/line.zig                 | shaper/coretext.rs:706-758                                                                                  | replaced       | CTLine glyph_count/glyph_runs via objc2; bounds/typographic APIs unused in src.                                                                                                               |
| pkg/macos/text/paragraph_style.zig      | —                                                                                                           | not-applicable | Unused in ghostty src (shaper uses kCTTypesetterOptionForcedEmbeddingLevel).                                                                                                                  |
| pkg/macos/text/run.zig                  | shaper/coretext.rs:765-826,1311-1379                                                                        | replaced       | getStatus -> CTRunStatus bit test; get*Ptr orelse get* -> NonNull check with copy helpers; len==0 yields an empty slice either way.                                                           |
| pkg/macos/text/stylized_strings.zig     | shaper/coretext.rs:70                                                                                       | replaced       | StringAttribute.font.key() -> kCTFontAttributeName (the only variant used).                                                                                                                   |
| pkg/macos/text/typesetter.zig           | shaper/coretext.rs:98-104,683-716                                                                           | replaced       | createWithAttributedStringAndOptions -> objc2; createLine's NULL->OutOfMemory preserved via a raw CTTypesetterCreateLine declaration.                                                         |

### Shard 37 — pkg/macos/video, pkg/oniguruma, pkg/opengl (2,277 lines, 29 units)

| Oracle unit                      | Rustty counterpart(s)                            | Status   | Note                                                                                                                                                                                                                             |
| -------------------------------- | ------------------------------------------------ | -------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| pkg/macos/video/c.zig            | metal/cvdisplaylink.rs:33-87                     | replaced | @cImport replaced by a hand-written CoreVideo extern block with exactly the 7 symbols used plus the typedefs.                                                                                                                    |
| pkg/macos/video/display_link.zig | metal/cvdisplaylink.rs                           | ported   | Error enum, create/release/start/stop/isRunning/setCurrentCGDisplay, and a monomorphized setOutputCallback trampoline matching the reference's inner callback. rustty adds an on-device #[ignore] KAT.                           |
| pkg/macos/video/pixel_format.zig | —                                                | missing  | The 83-member CVPixelFormatType enum has no counterpart; the oracle has no use site either — S37-F2.                                                                                                                             |
| pkg/oniguruma/build.zig          | crates-sys/oniguruma-sys/build.rs                | replaced | Same 47 C sources (+ a documented koi8.c addition), same config.h substitutions, same msvc flags, bindgen over oniguruma.h. Not reproduced: `-fsys=oniguruma` and installHeadersDirectory (build plumbing).                      |
| pkg/oniguruma/c.zig              | crates-sys/oniguruma-sys/wrapper.h; src/lib.rs   | replaced | Same single header; bindgen output re-exported and reachable as `rustty_oniguruma::c`.                                                                                                                                           |
| pkg/oniguruma/errors.zig         | crates/rustty-oniguruma/src/errors.rs            | ported   | MAX_ERROR_LEN, convert_error, error_string, ErrorInfo, the 83-variant Error enum and 82-entry code map compared entry-by-entry: identical.                                                                                       |
| pkg/oniguruma/init.zig           | crates/rustty-oniguruma/src/init.rs:17-28        | ported   | init/deinit identical.                                                                                                                                                                                                           |
| pkg/oniguruma/main.zig           | crates/rustty-oniguruma/src/lib.rs:23-53         | ported   | All re-exports present; refAllDecls is a Zig compile-smoke with no Rust analogue needed.                                                                                                                                         |
| pkg/oniguruma/match_param.zig    | crates/rustty-oniguruma/src/match_param.rs:18-38 | ported   | init/deinit/set_retry_limit_in_search match; `as c_ulong` cast noted — S37-F6 (refuted).                                                                                                                                         |
| pkg/oniguruma/regex.zig          | crates/rustty-oniguruma/src/regex.rs             | ported   | init/deinit/search/search_with_param/search_advanced(+_with_param) with identical argument order and errdefer semantics; the reference test ported with the same 3 assertions.                                                   |
| pkg/oniguruma/region.zig         | crates/rustty-oniguruma/src/region.rs            | ported   | repr(C) Region, deinit, count, iterator, starts/ends, Iterator::next identical.                                                                                                                                                  |
| pkg/oniguruma/testing.zig        | crates/rustty-oniguruma/src/testing.rs:38-40     | ported   | ensure_init with UTF-8 only; OnceLock replaces the reference's never-set flag (documented, same observable outcome).                                                                                                             |
| pkg/oniguruma/types.zig          | crates/rustty-oniguruma/src/types.rs             | ported   | All 31 encodings, all 12 syntaxes, Options with the same 25 bools in the same bit order; test "order" ported.                                                                                                                    |
| pkg/opengl/Buffer.zig            | rustty-renderer/src/opengl/gl/buffer.rs          | partial  | create/bind/destroy/bind_base, all Binding methods, data_info slice arm, Target/Usage newtypes match. NOT ported: Binding.attribute and dataInfo's `.one` arm — S37-F3/F4 (both refuted, uninstantiable/uncalled in the oracle). |
| pkg/opengl/Framebuffer.zig       | rustty-renderer/src/opengl/gl/framebuffer.rs     | ported   | create/destroy/bind, Target/Attachment/Status newtypes, unbind/texture_2d/renderbuffer/draw_buffers/check_status all match line-for-line.                                                                                        |
| pkg/opengl/Program.zig           | rustty-renderer/src/opengl/gl/program.rs         | partial  | create/create_vf/destroy/attach_shader/link/use_/uniform_block_binding/get_info_log match incl. log text and ordering. NOT ported: setUniform — S37-F1.                                                                          |
| pkg/opengl/Renderbuffer.zig      | rustty-renderer/src/opengl/gl/renderbuffer.rs    | ported   | create/destroy/bind (saves previous binding), unbind, storage with the c_int->GLenum widen.                                                                                                                                      |
| pkg/opengl/Sampler.zig           | rustty-renderer/src/opengl/gl/sampler.rs         | ported   | create/bind/parameter/destroy; the anytype `else => unreachable` arm becomes a compile-time constraint.                                                                                                                          |
| pkg/opengl/Shader.zig            | rustty-renderer/src/opengl/gl/shader.rs          | ported   | create/set_source_and_compile/get_info_log/destroy/display_info_log all match incl. log text.                                                                                                                                    |
| pkg/opengl/Texture.zig           | rustty-renderer/src/opengl/gl/texture.rs         | ported   | active/create/bind/destroy, all enums and newtypes checked constant-by-constant, all Binding methods match.                                                                                                                      |
| pkg/opengl/VertexArray.zig       | rustty-renderer/src/opengl/gl/vertex_array.rs    | ported   | create/bind/destroy plus all 7 Binding methods.                                                                                                                                                                                  |
| pkg/opengl/build.zig             | rustty-renderer/src/opengl/gl/sys.rs             | replaced | Zig module + vendored glad replaced by the `gl` crate loaded via gl::load_with.                                                                                                                                                  |
| pkg/opengl/c.zig                 | rustty-renderer/src/opengl/gl.rs                 | replaced | @cInclude("glad/gl.h") replaced by the `gl` crate's generated constants/entry points.                                                                                                                                            |
| pkg/opengl/draw.zig              | rustty-renderer/src/opengl/gl/draw.rs            | ported   | All 14 fns match including which ones call getError.                                                                                                                                                                             |
| pkg/opengl/errors.zig            | rustty-renderer/src/opengl/gl/errors.rs          | ported   | Error enum, get_error's 6 arms + Unknown fallback, must_error.                                                                                                                                                                   |
| pkg/opengl/extensions.zig        | rustty-renderer/src/opengl/gl/extensions.rs      | ported   | len, iterator, Iterator::next with the same bounds check and NUL slicing.                                                                                                                                                        |
| pkg/opengl/glad.zig              | rustty-renderer/src/opengl/gl/sys.rs             | replaced | Context/threadlocal/load/unload/version* replaced by the `gl` crate; version parsed from glGetString(GL_VERSION); `unload` has no analogue (only use is an errdefer).                                                            |
| pkg/opengl/main.zig              | rustty-renderer/src/opengl/gl.rs:27-115          | ported   | All re-exports one-for-one; `c`/`glad` deliberately not re-exported (documented).                                                                                                                                                |
| pkg/opengl/primitives.zig        | rustty-renderer/src/opengl/gl/primitives.rs      | ported   | repr(i32) Primitive with the same 5 GL constants; commented-out Metal-parity exclusions carried as comments.                                                                                                                     |

### Shard 38 — pkg/sentry, pkg/simdutf, pkg/spirv-cross, pkg/wuffs (1,363 lines, 18 units)

| Oracle unit               | Rustty counterpart(s)                                               | Status   | Note                                                                                                                                                                                                                                                           |
| ------------------------- | ------------------------------------------------------------------- | -------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| pkg/sentry/build.zig      | crates-sys/sentry-sys/build.rs; Cargo.toml                          | replaced | Identical 27-entry srcs list, same unwinder defines, same per-OS symbolizer/unwinder/path/pageallocator/modulefinder files, transport none + breakpad backend (the product configuration). Only divergence: the unsupported-OS hard error is omitted — S38-F4. |
| pkg/sentry/c.zig          | rustty-crash/src/sentry_pkg/c.rs; sentry-sys/wrapper.h; src/lib.rs  | ported   | @cImport(sentry.h) -> bindgen of the same header, re-exported as sentry_pkg::c.                                                                                                                                                                                |
| pkg/sentry/envelope.zig   | rustty-crash/src/sentry_pkg/envelope.rs                             | ported   | deinit/writeToFile/serialize/event with the same semantics; `&str` narrowing noted — S38-F3 (refuted).                                                                                                                                                         |
| pkg/sentry/level.zig      | rustty-crash/src/sentry_pkg/level.rs                                | ported   | repr(i32) with the same 5 variants and values.                                                                                                                                                                                                                 |
| pkg/sentry/main.zig       | rustty-crash/src/sentry_pkg/mod.rs                                  | ported   | All re-exports; captureEvent/setContext/removeContext/setTag/free present; refAllDecls has a counterpart.                                                                                                                                                      |
| pkg/sentry/transport.zig  | rustty-crash/src/sentry_pkg/transport.rs                            | ported   | SendFunc/FreeFunc hoisted to module level; init/deinit/setState/setStateFreeFunc identical.                                                                                                                                                                    |
| pkg/sentry/uuid.zig       | rustty-crash/src/sentry_pkg/uuid.rs                                 | ported   | init/isNil/string identical.                                                                                                                                                                                                                                   |
| pkg/sentry/value.zig      | rustty-crash/src/sentry_pkg/value.rs                                | ported   | All constructors, decref/incref, isNull, set, get identical.                                                                                                                                                                                                   |
| pkg/simdutf/build.zig     | crates-sys/simdutf-sys/build.rs                                     | replaced | cc build of the vendored amalgamation with the product's no_libcxx set, C++17, vendor include exported. MSVC-only no_libcxx defines/flags not mirrored — S38-F5.                                                                                               |
| pkg/spirv-cross/build.zig | crates-sys/spirv-cross-sys/build.rs                                 | replaced | Same 7 TUs, same -D flags and ubsan opt-outs, -fPIC on freebsd/musl, upstream root as include path. `-fsys=spirv-cross` has no equivalent — S38-F6.                                                                                                            |
| pkg/spirv-cross/c.zig     | crates-sys/spirv-cross-sys/wrapper.h; src/lib.rs                    | ported   | bindgen over the same header with prepend_enum_name(false).                                                                                                                                                                                                    |
| pkg/spirv-cross/main.zig  | crates-sys/spirv-cross-sys/src/lib.rs                               | ported   | The single `pub const c` re-export is the crate root's include!(bindings.rs).                                                                                                                                                                                  |
| pkg/wuffs/build.zig       | crates-sys/wuffs-sys/build.rs; wrapper.h; include/{stdlib,string}.h | replaced | Verbatim 10-entry module define list, -DWUFFS_IMPLEMENTATION on the single TU, windows ubsan opt-outs, bindgen over the same header. no-libc shim headers mirrored but unused.                                                                                 |
| pkg/wuffs/src/error.zig   | crates/rustty-wuffs/src/error.rs                                    | ported   | Error atoms and check() semantics; log scope folded into the message text.                                                                                                                                                                                     |
| pkg/wuffs/src/jpeg.zig    | rustty-wuffs/src/jpeg.rs + 3 fixtures                               | ported   | Full decode pipeline with identical guards and error mapping; 3 tests ported with the same assertions (fixture provenance differs — S38-F1).                                                                                                                   |
| pkg/wuffs/src/main.zig    | crates/rustty-wuffs/src/lib.rs                                      | ported   | Re-exports, maximum_image_size = 4 GiB, ImageData; the no-libc weak stub block is dead under cc+system libc; refAllDeclsRecursive has a counterpart.                                                                                                           |
| pkg/wuffs/src/png.zig     | rustty-wuffs/src/png.rs; src/test_fixtures.rs                       | ported   | Body identical to jpeg with png symbols; 3 tests ported with the same assertions.                                                                                                                                                                              |
| pkg/wuffs/src/swizzle.zig | crates/rustty-wuffs/src/swizzle.rs                                  | ported   | All 6 public converters plus the private swizzle with all 7 asserts; both reference tests ported with identical expected bytes plus extra KATs. Prepare-failure log text differs — S38-F2.                                                                     |

### Shard 39 — pkg/zlib, src/App.zig, src/Command.zig (1,764 lines, 3 units)

| Oracle unit              | Rustty counterpart(s)                                                                              | Status   | Note                                                                                                                                                                                                                                                                                                                                    |
| ------------------------ | -------------------------------------------------------------------------------------------------- | -------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| pkg/zlib/build.zig[1-77] | crates-sys/zlib-sys/{build.rs,Cargo.toml,vendor/,src/lib.rs}                                       | replaced | cc-based -sys crate (links="z", vendored verbatim zlib 1.3.1, cargo:include exported as DEP_Z_INCLUDE for libpng-sys/freetype-sys). 10 of 15 TUs compiled; gz*/infback not vendored — S39-F5. The omitted HAVE_*/Z_HAVE_UNISTD_H defines are unreferenced or width-identical on LP64.                                                   |
| src/App.zig[1-653]       | crates/rustty-core/src/app.rs                                                                      | ported   | Full file ported (App/Message/NewWindow/OpenConfig/Queue/Mailbox + wasm block). Every fn, field default, switch arm and the quit short-circuit compared branch-for-branch: identical. App.zig has 0 test blocks; app.rs adds 13 seam tests. Only divergence: log.* sites are comments — S39-F6.                                         |
| src/Command.zig[1-1034]  | rustty-termio/src/command.rs; rustty-apprt/src/gtk/app.rs; rustty-termio/tests/rt_hooks_install.rs | ported   | RELOCATED to rustty-termio (R4 cycle). All fields/defaults, Exit, RtPreExec/RtPostFork hooks (comptime @hasDecl -> install_rt_hooks), and every start/fork/setupFd/wait/env helper present; all 10 test blocks ported with the same assertions. Divergences in the child exec-failure path and setupFd errno handling: S39-F1/F2/F3/F4. |

**Partially-read units: none.** Every unit listed by every shard was read in full (`units_partial` was empty for all 9 shards).

## Findings

### High

#### S34-F1 — x86 AVX2 gate drops the XCR0 (xgetbv) OS-support check and the LZCNT/BMI/BMI2 requirements

- **Status: CONFIRMED** (neither lens refuted)
- Kind: behavioral-difference · Severity: high
- Oracle: `pkg/highway/src/detect/x86.zig:63-94`
- Rustty: `crates-sys/highway-sys/cpp/detect.c:172-183`

The oracle enables avx2 only if (a) CPUID.1:ECX has XSAVE(27) and AVX(28), (b) XGETBV(0) reports SSE+AVX state enabled by the OS (`(xcr0 & 0x6) == 0x6`), (c) leaf7.ebx AVX2(5), leaf1.ecx FMA(12)+F16C(29), and (d) extended leaf 0x80000001 ecx LZCNT(5) plus leaf7.ebx BMI(3) and BMI2(8). The C port checks only the CPUID bits AVX2/FMA/F16C/XSAVE/AVX and never executes xgetbv nor checks LZCNT/BMI/BMI2 (the comment at detect.c:172 admits it is a "subset"). Highway's HWY_AVX2 target codegen assumes BMI/BMI2/LZCNT and OS-enabled AVX state, so the two implementations return different masks and rustty can dispatch AVX2 kernels the oracle would refuse — up to a SIGILL.

Evidence: x86.zig:64-70 computes xcr0 via `asm volatile ("xgetbv")` and `has_avx_save = (xcr0 & 0x6) == 0x6`; x86.zig:87-93 requires `bit(leaf_ext.ecx,5) and bit(leaf7.ebx,3) and bit(leaf7.ebx,8)`. detect.c:173-183 has no xgetbv, no cpuid(0x80000001), and tests only l7b bit5, l1c bits 12,29,27,28. Workspace grep for `xgetbv` outside vendored headers: no hits.

Suggested fix: port x86.zig:63-94 literally — read XCR0 with `_xgetbv(0)` (or inline asm) when OSXSAVE&&AVX, require `(xcr0&0x6)==0x6`, then require leaf 0x80000001 ecx bit5 and leaf7 ebx bits 3 and 8 before setting TGT_AVX2; gate the whole block on `has_avx_save && max_leaf>=7` as the oracle does.

#### S34-F4 — PowerPC target detection (ppc8/ppc9/ppc10) has no counterpart; powerpc falls to `return 0`

- **Status: CONFIRMED**
- Kind: missing-file · Severity: high
- Oracle: `pkg/highway/src/detect/ppc.zig:1-43`
- Rustty: `crates-sys/highway-sys/cpp/detect.c:188-190`

detect.zig:27 routes .powerpc/.powerpc64/.powerpc64le to ppc.detect(), which on Linux reads AT_HWCAP/AT_HWCAP2 and sets ppc8 when ALTIVEC(0x10000000)+VSX(0x80)+ARCH_2_07(0x80000000)+VEC_CRYPTO(0x02000000), ppc9 additionally on ARCH_3_00(0x00800000), ppc10 additionally on ARCH_3_1(0x00040000)+MMA(0x00020000); non-Linux returns 0. detect.c's `#if` ladder has only aarch64 and x86 branches; on powerpc the `#else` stub returns 0 unconditionally, so Highway would dispatch only the baseline/EMU128 target where the oracle dispatches PPC8/9/10. Note this arch _is_ reachable: HWY_HAVE_RUNTIME_DISPATCH_LINUX covers PPC on Linux.

Evidence: ppc.zig:24-40 nested ppc8/ppc9/ppc10 logic; detect.c:188-190 `#else int64_t rustty_hwy_detect_targets(void) { return 0; } #endif`; no TGT_PPC* define exists anywhere in the workspace (oracle targets.zig:95-97 asserts bits 47,48,49).

Suggested fix: add an `#elif defined(__powerpc__) || defined(__powerpc64__)` branch porting ppc.zig with TGT_PPC10 (1<<47), TGT_PPC9 (1<<48), TGT_PPC8 (1<<49), gated on `__linux__`.

#### S34-F5 — s390x target detection (z14/z15) has no counterpart; s390x falls to `return 0`

- **Status: CONFIRMED**
- Kind: missing-file · Severity: high
- Oracle: `pkg/highway/src/detect/s390x.zig:1-29`
- Rustty: `crates-sys/highway-sys/cpp/detect.c:188-190`

detect.zig:28 routes .s390x to s390x.detect(): on Linux, if AT_HWCAP has VX(1<<11) and VXE(1<<13) set z14, and additionally VXE2(1<<15) set z15; non-Linux returns 0. detect.c has no s390x branch; the `#else` stub returns 0, so the mask differs on every z14+/z15+ machine (and S390X on Linux is inside Highway's runtime-dispatch gate).

Evidence: s390x.zig:14-26; detect.c:188-190; no TGT_Z14/TGT_Z15 defines (oracle targets.zig:98-99 asserts bits 50,51). Workspace grep for `z14`/`z15` outside vendored code: nothing.

Suggested fix: add an `#elif defined(__s390x__)` branch porting s390x.zig with TGT_Z15 (1<<50), TGT_Z14 (1<<51).

#### S34-F6 — RISC-V target detection (rvv) has no counterpart; riscv falls to `return 0`

- **Status: CONFIRMED**
- Kind: missing-file · Severity: high
- Oracle: `pkg/highway/src/detect/riscv.zig:1-22`
- Rustty: `crates-sys/highway-sys/cpp/detect.c:188-190`

detect.zig:29 routes .riscv32/.riscv64 to riscv.detect(): on Linux sets rvv when AT_HWCAP bit `('V'-'A')` = bit 21 is set; non-Linux returns 0. detect.c has no riscv branch and returns 0, so on RVV-capable hardware the oracle's mask has bit 37 set and rustty's does not.

Evidence: riscv.zig:15-19 `HWCAP_V = 1 << ('V' - 'A'); if (hwcap & HWCAP_V != 0) t.rvv = true;`; detect.c:188-190; no TGT_RVV define (targets.zig:88 asserts bit 37).

Suggested fix: add an `#elif defined(__riscv)` branch porting riscv.zig with TGT_RVV (1<<37).

### Medium

#### S31-F1 — FreeBSD fontconfig directory defines not ported (Linux paths used on FreeBSD)

- **Status: CONFIRMED**
- Kind: missing-branch · Severity: medium
- Oracle: `pkg/fontconfig/build.zig:178-190`
- Rustty: `crates-sys/fontconfig-sys/build.rs:210-217`

The reference selects FC_TEMPLATEDIR=/usr/local/etc/fonts/conf.avail, FONTCONFIG_PATH=/usr/local/etc/fonts, CONFIGDIR=/usr/local/etc/fonts/conf.d when the target OS is FreeBSD, and the /usr/share + /etc/fonts + /usr/local/fontconfig/conf.d set otherwise. The rustty build.rs unconditionally emits the non-FreeBSD set for every non-Windows target (the comment even labels it "Non-FreeBSD (Linux) arm"), so a FreeBSD build of the vendored fontconfig would look for its config in /etc/fonts instead of /usr/local/etc/fonts — an observable font-discovery difference on that OS. Note the crate _did_ mirror the Windows arm "for completeness", so this is a genuine gap, not a scoped-out target policy; other -sys build scripts (breakpad, sentry, glslang, spirv-cross, dcimgui) do have freebsd handling.

Evidence: `grep -ni freebsd build.rs` matches only the comment on line 210.

Suggested fix: add `if target_os == "freebsd" { define FC_TEMPLATEDIR=…conf.avail, FONTCONFIG_PATH=/usr/local/etc/fonts, CONFIGDIR=…conf.d } else { existing }` in the non-Windows arm.

#### S34-F2 — x86 AVX-512 (avx3/avx3_dl/avx3_zen4/avx3_spr) and AVX10 detection, plus isAMD(), are not ported

- **Status: CONFIRMED** (severity corrected high -> medium by both lenses)
- Kind: missing-branch · Severity: medium
- Oracle: `pkg/highway/src/detect/x86.zig:72-76, 96-154, 160-166`
- Rustty: `crates-sys/highway-sys/cpp/detect.c:172-186`

The oracle probes leaf7 ebx/ecx/edx and leaf7 sub-1 for AVX512F/VL/DQ/BW/CD (avx3), VNNI/VPCLMULQDQ/VBMI/VBMI2/VAES/VPOPCNTDQ/BITALG/GFNI (avx3_dl), AVX512BF16 on AMD (avx3_zen4 via an isAMD vendor-string check), AVX512FP16+BF16 (avx3_spr), and AVX10.1-512 via leaf 0x24, all gated by has_avx512_save (Darwin: always true; else `(xcr0 & 0xE0) == 0xE0`). None of this exists in detect.c: TGT_AVX3 is defined at detect.c:21 but never set, and there are no defines for avx3_dl(7)/avx3_zen4(6)/avx3_spr(4). On any AVX-512 CPU the oracle's mask has bits 4-8 set where rustty's has none, so Highway's runtime dispatch chooses a different (AVX2) target. Severity corrected to medium: the divergence is confined to the exported mask and to which result-identical Highway kernel is dispatched.

Suggested fix: port the AVX-512 and AVX10 blocks and isAMD() from x86.zig verbatim, adding TGT_AVX3_DL (1<<7), TGT_AVX3_ZEN4 (1<<6), TGT_AVX3_SPR (1<<4) and the has_avx512_save gate.

#### S34-F3 — aarch64 Linux never sets sve / sve_256 / sve2 / sve2_128; SVE vector length is probed and discarded

- **Status: CONFIRMED** (severity corrected high -> medium by both lenses)
- Kind: missing-branch · Severity: medium
- Oracle: `pkg/highway/src/detect/aarch64_linux.zig:19-20, 33-50`
- Rustty: `crates-sys/highway-sys/cpp/detect.c:111-115`

When HWCAP_SVE is set the oracle uses sveVectorBytes() to set sve (>=32 bytes) and sve_256 (==32), and when HWCAP2_SVE2 and HWCAP2_SVEAES are both set, sets sve2 (>=32) or sve2_128 (==16). The C port calls `sve_vector_bytes()` only for its side effect (`(void)sve_vector_bytes()`) and sets no SVE bit; the HWCAP2_SVE2 (1<<1) and HWCAP2_SVEAES (1<<2) constants are absent and there are no TGT_SVE* defines. The comment at detect.c:111-112 documents this as intentional, but the returned mask still differs from the oracle on any SVE-capable Linux machine.

Suggested fix: add TGT_SVE2_128 (1<<18), TGT_SVE_256 (1<<19), TGT_SVE2 (1<<23), TGT_SVE (1<<24), HWCAP2_SVE2_ (1UL<<1), HWCAP2_SVEAES_ (1UL<<2) and port the block literally using the sve_vector_bytes() result.

#### S36-F1 — IOSurface Display P3 color space (createNamed(.displayP3) + setColorSpace) is a deferred seam

- **Status: CONFIRMED** (severity corrected high -> medium by the semantic lens: no live on-screen presentation path yet, so the difference is latent)
- Kind: stub-or-unimplemented · Severity: medium
- Oracle: `pkg/macos/iosurface/iosurface.zig:19,47-49,73-84` (with `pkg/macos/graphics/color_space.zig:21-26`, use-site `src/renderer/metal/Target.zig:51-60`)
- Rustty: `crates/rustty-renderer/src/metal/target.rs:80-98,145`

The reference Target.init creates a Display P3 CGColorSpace, passes it as IOSurface.init's `colorspace` property, and IOSurface.init calls setColorSpace, which serializes it with CGColorSpaceCopyPropertyList and tags the surface via IOSurfaceSetValue(kIOSurfaceColorSpace) — this is what gives the on-screen surface "Apple-style" alpha blending in the display color space. rustty's Target::init contains only a comment marking this "R5 DEFERRED SEAM … SKIPPED here", and a workspace-wide grep finds no kIOSurfaceColorSpace / CGColorSpaceCopyPropertyList / IOSurfaceSetValue anywhere in crates/ or macos/. (The `use_display_p3` shader uniform is a different mechanism, not the IOSurface tag.)

Suggested fix: in Target::init, after `IOSurfaceRef::new`, create CGColorSpace::with_name(kCGColorSpaceDisplayP3), call CGColorSpaceCopyPropertyList and IOSurfaceSetValue(surface, kIOSurfaceColorSpace, plist), releasing the plist afterwards — i.e. port iosurface.zig:73-84 as an IOSurface helper in target.rs.

#### S36-F3 — NULL results from CTFont/CTFontDescriptor/CTFontCollection creators become objc2 panics instead of `error.OutOfMemory` / `catch return null`

- **Status: CONFIRMED**
- Kind: missing-error-path · Severity: medium
- Oracle: `pkg/macos/text/font.zig:10-19,44-59` (also font_descriptor.zig:7-47, font_collection.zig:15-23, use-site `src/apprt/embedded.zig:2434-2438`)
- Rustty: `crates/rustty-font-shaper/src/face/coretext.rs:212,227,354,363,428,444,456,1364` (also discovery.rs:1156,1225,1654,2057,2065; shaper/coretext.rs:1063,1071; rustty-apprt-c/src/lib.rs:2921-2925)

The reference wrappers createWithFontDescriptor, copyWithAttributes, FontDescriptor.createWithAttributes/createCopyWithAttributes/createCopyWithVariation and FontCollection.createWithFontDescriptors all map a NULL CoreText result to `Allocator.Error.OutOfMemory`, and every ghostty use-site propagates it with `try`. rustty calls objc2-core-text's non-null wrappers, which `expect(...)` and panic on NULL. The most visible case is `ghostty_surface_quicklook_font`: the reference does `face.font.copyWithAttributes(...) catch return null` so the host gets NULL, whereas rustty-apprt-c panics across the C ABI. rustty already re-binds raw symbols for the nullable cases it noticed (CTFontCreateForString, CTFontCreateWithName, CTFontCopyDisplayName, CTTypesetterCreateLine, CGBitmapContextCreate) but not these.

Evidence: objc2-core-text-0.3.2 CTFont.rs:616-628 / CTFontDescriptor.rs:467-548,627-640 / CTFontCollection.rs:155-167 all `ret.expect("function was marked as returning non-null, but actually returned NULL")`.

Suggested fix: declare the raw CTFontCreateWithFontDescriptor / CTFontCreateCopyWithAttributes / CTFontDescriptorCreateWithAttributes / CTFontDescriptorCreateCopyWithAttributes / CTFontDescriptorCreateCopyWithVariation / CTFontCollectionCreateWithFontDescriptors symbols in the existing `sys` modules (same pattern as CTFontCreateWithName) returning `*mut T`, and map NULL to Error::OutOfMemory (or `return null` in the apprt-c export).

#### S36-F5 — No test for the "from descriptors no match" branch (NULL matching-descriptor array -> empty list)

- **Status: CONFIRMED**
- Kind: missing-test · Severity: medium
- Oracle: `pkg/macos/text/font_collection.zig:117-139`
- Rustty: `crates/rustty-font-shaper/src/discovery.rs:1520-1547` (tests module 2590-2900)

The reference wrapper deliberately diverges from CoreText (NULL -> empty array) and pins that with test "from descriptors no match" (descriptor "ThisShouldNeverExist" -> `list.getCount() == 0`). rustty re-implements that divergence _itself_ in `copy_matching_descriptors` (`None => return result`, with a PORT NOTE) — it is now rustty logic, not objc2's — but no rustty test exercises a non-matching descriptor through discover()/copy_matching_descriptors. `grep -rni 'ThisShouldNeverExist|no match|nonexistent'` over rustty-font-shaper/src returns only unrelated web_canvas.rs comments.

Suggested fix: add a discovery test that calls CoreText::discover with family "ThisShouldNeverExist" and asserts the iterator yields nothing.

#### S39-F1 — PATH search: EPERM from execve continues the search in rustty but aborts it in the oracle

- **Status: CONFIRMED**
- Kind: behavioral-difference · Severity: medium
- Oracle: `src/Command.zig:226-256`
- Rustty: `crates/rustty-termio/src/command.rs:1518-1650` (specifically :1533 and :1640-1644)

The oracle's inline PATH loop switches on the RAW errno: only `.ACCES` sets seen_eacces and continues; `.NOENT/.NOTDIR` continue; every other errno (including EPERM, E2BIG, ENOMEM…) breaks out immediately with that errno. rustty first maps the errno through a Zig-std-style `ExecveError` (`libc::EPERM => ExecveError::AccessDenied`, command.rs:1533) and then matches `ExecveError::AccessDenied => seen_eacces = true` (command.rs:1641). Consequently an EPERM from one PATH candidate is treated like EACCES: the search continues to later PATH entries — possibly exec'ing a _different_ binary the oracle would never reach — and, if nothing else execs, the final report is "AccessDenied" instead of the oracle's immediate EPERM abort. The errno->variant collapse also loses E2BIG/ENOMEM, EACCES/EPERM and EIO/ELOOP distinctions in the reported text.

Suggested fix: have `execvpeZ_search` operate on raw errno values like the oracle (or add a distinct `PermissionDenied`/EPERM variant that falls into the `else => break` arm), and carry the raw errno into the report.

### Low

#### S33-F1 — setNamespace use site converts the raw byte config value through from_utf8_lossy

- **Status: CONFIRMED**
- Kind: behavioral-difference · Severity: low
- Oracle: `pkg/gtk4-layer-shell/src/main.zig:67-69` (use site `src/apprt/gtk/winproto/wayland.zig:352`)
- Rustty: `crates/rustty-apprt/src/gtk/winproto/wayland.rs:907-909`

The oracle passes the `gtk-quick-terminal-namespace` config value as a raw NUL-terminated byte string straight to gtk_layer_set_namespace. rustty does `window.set_namespace(Some(&String::from_utf8_lossy(&config.gtk_quick_terminal_namespace)))`, so a namespace containing invalid UTF-8 bytes is rewritten with U+FFFD before reaching the compositor rather than passed verbatim. The field really is raw bytes (`crates/rustty-config/src/config_struct.rs:3789 pub gtk_quick_terminal_namespace: Vec<u8>`), so the case is reachable; the only use site in the workspace is wayland.rs:910 and there is no byte-preserving path.

Suggested fix: call the -sys function (`gtk4_layer_shell_sys::gtk_layer_set_namespace`) with a CString built from the raw bytes, or document that the crate's `&str` API forces the lossy conversion.

#### S35-F2 — Metal error use-site of String.cstringPtr drops the `metal error={s}` log

- **Status: CONFIRMED**
- Kind: behavioral-difference · Severity: low
- Oracle: `pkg/macos/foundation/string.zig:70-77` (use sites `src/renderer/metal/shaders.zig:446`, `src/renderer/metal/Pipeline.zig:202`)
- Rustty: `crates/rustty-renderer/src/metal/pipeline.rs:190-193`, `shaders.rs:435,445`

The only ghostty src consumers of `String.cstringPtr(.ascii)` are the Metal `checkError` helpers, which log the NSError's localizedDescription (`log.err("metal error={s}", …)`) before returning `error.MetalFailed`. Rustty maps the objc2 `Err(nserr)` straight to `Error::MetalFailed` with `|_nserr|`, discarding the description; the PORT NOTE at pipeline.rs:182-189 admits "no logging facade is wired in the port yet" (the crate has no `log` dependency at all). The error value is identical; only the diagnostic text is lost.

Suggested fix: emit the reference log line (`log::error!("metal error={}", nserr.localizedDescription())`) in both map_err closures once a logging seam exists.

#### S37-F5 — Monomorphized output_callback drops the reference `log.err("error notifying draw_now err={}")`

- **Status: CONFIRMED**
- Kind: behavioral-difference · Severity: low
- Oracle: `pkg/macos/video/display_link.zig:51-83` (callback body `src/renderer/generic.zig:986-994`)
- Rustty: `crates/rustty-renderer/src/metal/cvdisplaylink.rs:282-292`

The reference setOutputCallback forwards to the caller-supplied callbackFn; the single instantiation logs `log.err("error notifying draw_now err={}", .{err})` when `draw_now.notify()` fails. rustty inlines that callback body into the fixed trampoline but the `if let Err(_err) = draw_now.notify() { /* comment only */ }` branch emits nothing — a notify failure on the display-link tick is silently swallowed.

Suggested fix: emit the error log through the renderer crate's log seam inside the Err arm.

#### S38-F2 — swizzle prepare-failure warning text/scope differs (log text only)

- **Status: CONFIRMED**
- Kind: behavioral-difference · Severity: low
- Oracle: `pkg/wuffs/src/swizzle.zig:172-176` (scope declared at :7)
- Rustty: `crates/rustty-wuffs/src/swizzle.rs:345-350` -> `crates/rustty-wuffs/src/error.rs:41-52`

On `wuffs_base__pixel_swizzler__prepare` failure the reference logs the raw status message under the `.wuffs_swizzler` scope (`log.warn("{s}", .{e})`). The port routes through `crate::error::check`, which prints `warning(wuffs): decode err=<msg>` — different scope tag _and_ an added `decode err=` prefix the swizzler branch does not have. Returned error (`WuffsError`) is identical. Likewise png.rs/jpeg.rs fold the `.wuffs_png`/`.wuffs_jpeg` scopes into the message.

Suggested fix: emit the swizzler-scoped message form, or give `check` a scope parameter as the reference's `log: anytype` does.

#### S38-F4 — Unsupported OS silently builds without a module finder instead of failing the build

- **Status: CONFIRMED**
- Kind: missing-error-path · Severity: low
- Oracle: `pkg/sentry/build.zig:119-124`
- Rustty: `crates-sys/sentry-sys/build.rs:156-171`

The reference's modulefinder switch returns `error.UnsupportedTarget` (with a warning) for any OS other than windows/macos/ios/linux/freestanding. The Rust build script's `_ => {}` arm just omits the modulefinder object, so a build for e.g. FreeBSD proceeds and may fail later at link time (unresolved `sentry__modulefinder_*`) or produce a client without module enumeration. Build-plumbing only; unreachable on rustty's supported targets.

Suggested fix: `panic!("target_os={target_os} not supported")` in the fallback arm (freestanding excepted) to mirror the reference's hard error.

#### S38-F5 — simdutf MSVC-specific no_libcxx flags not mirrored

- **Status: CONFIRMED**
- Kind: missing-branch · Severity: low
- Oracle: `pkg/simdutf/build.zig:51-72`
- Rustty: `crates-sys/simdutf-sys/build.rs:47-53`

With `no_libcxx` (the product setting) the reference adds, on the MSVC ABI only, `-D_USE_STD_VECTOR_ALGORITHMS=0`, `-D_Avx2WmemEnabledWeakValue=_Avx2WmemEnabled`, `-fno-autolink`, `-fno-stack-protector` and `-std=c++17`. simdutf-sys applies SIMDUTF_NO_LIBCXX/-fno-exceptions/-fno-rtti and C++17 unconditionally but never the four MSVC defines/flags, so an x86_64-pc-windows-msvc build of the vendored amalgamation could pull in the UCRT/vector-algorithm symbols the reference deliberately avoids. Grep of the whole build.rs for `msvc`/`TARGET_ENV` returns nothing.

Suggested fix: add `if CARGO_CFG_TARGET_ENV == "msvc" { define _USE_STD_VECTOR_ALGORITHMS=0, _Avx2WmemEnabledWeakValue=_Avx2WmemEnabled; flag -fno-autolink, -fno-stack-protector }`.

#### S39-F2 — Child exec-failure stderr text prints Zig-std error names instead of the errno tag

- **Status: CONFIRMED**
- Kind: behavioral-difference · Severity: low
- Oracle: `src/Command.zig:258-278` (message at :270-274)
- Rustty: `crates/rustty-termio/src/command.rs:713-741, 1479-1516`

On a non-ENOENT exec failure the oracle writes `exec syscall failed with unexpected error: E{s}` with `@tagName(e)` of the raw errno (e.g. `EACCES`, `ENOEXEC`, `ENAMETOOLONG`). rustty writes `{err}` where `err` is `ExecveError`'s Display, i.e. `error.AccessDenied`, `error.InvalidExe`, `error.NameTooLong`, `error.Unexpected`. This text is user-visible in the terminal when a configured command fails to exec. The ENOENT message matches exactly.

Suggested fix: format the raw errno as `E<NAME>` (a small errno->name table, or carry the errno and print via libc name lookup) to match the oracle text.

#### S39-F4 — setupFd (macOS/FreeBSD/iOS): fcntl/dup2 errno handling differs from the oracle's PosixCall wrapper

- **Status: CONFIRMED**
- Kind: behavioral-difference · Severity: low
- Oracle: `src/Command.zig:483-533` (PosixCall.f at :440-458)
- Rustty: `crates/rustty-termio/src/command.rs:1253-1275, 1355-1367`

The oracle routes fcntl(F_GETFD), fcntl(F_SETFD) and dup2 through one `PosixCall.f` wrapper: EINTR retries, EAGAIN/EACCES->Locked, EBUSY->FileBusy (NO retry), EPERM->PermissionDenied, EMFILE->ProcessFdQuotaExceeded, EDEADLK->DeadLock, ENOLCK->LockedRegionLimitExceeded, else unexpectedErrno. rustty issues the two fcntl calls bare with no EINTR retry and maps every failure to `Error::Unexpected(errno)`; `posix_dup2` loops on `EBUSY|EINTR` (old Zig-std dup2 semantics) where the oracle returns `error.FileBusy` immediately, and maps EMFILE only. The caller collapses every setupFd error into ExecFailedInChild, so the observable divergence is retry-vs-fail on dup2 EBUSY and fcntl EINTR (both practically unreachable on Darwin). The Linux dup3 arm matches exactly.

Suggested fix: port the PosixCall wrapper as a local `posix_call(|| …)` with the oracle's errno switch, used by fcntl and dup2 on the Darwin/FreeBSD arm.

#### S39-F5 — zlib-sys omits gzclose/gzlib/gzread/gzwrite/infback translation units

- **Status: CONFIRMED**
- Kind: missing-file · Severity: low
- Oracle: `pkg/zlib/build.zig:61-77`
- Rustty: `crates-sys/zlib-sys/build.rs:43-62`

The oracle's `srcs` compiles all 15 zlib units. zlib-sys vendors and compiles 10 (adler32, crc32, inftrees, inffast, inflate, uncompr, deflate, trees, compress, zutil); gzclose.c, gzlib.c, gzread.c, gzwrite.c and infback.c are neither vendored nor compiled, so the `gz*` API and `inflateBack*` are absent from the emitted `libz`, despite build.rs claiming "FULL zlib" (lines 25, 41). Verified no in-tree consumer needs them (freetype ftgzip with SYSTEM_ZLIB, libpng, sentry use inflate/deflate/crc32 only; libxml2 is zlib=false in both trees), so this is a linkable-symbol surface gap rather than a runtime break.

Suggested fix: vendor the five remaining verbatim 1.3.1 units and append them to SOURCES, or correct the "FULL zlib" doc comment to state the subset.

#### S39-F6 — All App.zig log.debug/info/warn sites are comment-only in app.rs (no log output)

- **Status: CONFIRMED**
- Kind: behavioral-difference · Severity: low
- Oracle: `src/App.zig:80-81, 94-95, 168, 199, 224, 265-268, 274, 348, 396-399, 484-487, 513-523`
- Rustty: `crates/rustty-core/src/app.rs:233, 253, 353, 407, 445, 489-495, 519, 590, 640-643, 800-803, 826-831`

Every std.log site in App.zig (warmup spawn failures, conditional-state apply failure, quit-timer start/stop errors, mailbox message debug, "quit message received, short circuiting mailbox drain", focus event, keybind/chained/surface action errors) is preserved verbatim as a comment with no runtime emission. This is the project-wide rustty convention (no logging shim anywhere in crates/*/src), so the observable difference is the absence of these diagnostics. Note also the performAllAction comment at app.rs:828-831 reflects the older `ptr={X}` text, whereas the pinned oracle logs `id={x}` with `surface.core().id` (App.zig:518-523 — the finding's original citation of 484-487 points at performAllChainedAction).

Suggested fix: emit these sites once a logging shim is adopted project-wide; at minimum update the performAllAction comment to the pinned `id={x}` form.

#### S31-F2 — Windows+libxml2 `-Werror=implicit-function-declaration` flag not mirrored

- **Status: DISPUTED** (refuted by the **semantic** lens)
- Kind: missing-branch · Severity: low
- Oracle: `pkg/fontconfig/build.zig:223-228`
- Rustty: `crates-sys/fontconfig-sys/build.rs:227-234`

When libxml2 is enabled and the target is Windows the reference adds `-Werror=implicit-function-declaration`. The rustty build.rs adds the three ENABLE_LIBXML2 defines but never this flag (and additionally passes `-w`, which silences all warnings). Windows is not a supported rustty target and the flag affects diagnostics only.

Refuting lens — **semantic**: "build.zig:223-228 adds `-Werror=implicit-function-declaration` only on Windows inside the libxml2 block; rustty build.rs:227-234 omits it. This is a compiler-diagnostic flag: it changes whether a warning becomes an error, not generated code or runtime behaviour. Moreover the oracle itself already passes -Wno-implicit-function-declaration globally (build.zig:122-124, mirrored at build.rs:153), so even in the oracle the later/earlier flag interaction is diagnostics-only. No input/state yields a different observable result; the finder's own text concedes 'no runtime behaviour changes'."

Suggested fix: add `if windows { build.flag_if_supported("-Werror=implicit-function-declaration"); }` after the libxml2 defines for completeness.

#### S31-F3 — ImGui_ImplOSX_{Init,Shutdown,NewFrame} externs and imgui_impl_osx.mm backend not present

- **Status: DISPUTED** (refuted by the **semantic** lens)
- Kind: missing-function · Severity: low
- Oracle: `pkg/dcimgui/main.zig:29-32` (backend compiled via build.zig:143-154)
- Rustty: `crates-sys/dcimgui-sys/src/lib.rs:154-330`; omission documented at `Cargo.toml:19-21`

The reference declares the three OSX platform-backend entry points and compiles `backends/imgui_impl_osx.mm` into the library on macOS. rustty declares no ImGui_ImplOSX_* symbols and intentionally does not compile the backend. No ghostty source outside pkg/ references ImGui_ImplOSX_*, so no use-site behaviour is lost; the binding surface is narrower than the reference's.

Refuting lens — **semantic**: "Confirmed main.zig:29-32 declares ImGui_ImplOSX_{Init,Shutdown,NewFrame} and that rustty declares none. But `grep -rn ImplOSX src macos` in the oracle checkout returns zero hits, so no oracle use-site ever calls these symbols; the macOS inspector path goes through the Metal backend (mirrored at crates-sys/dcimgui-sys/src/lib.rs:194-250 with cfg(target_os=\"macos\")). A never-called extern declaration plus an uncompiled backend .mm produces no observable behavioural difference — unreachable-code-only divergence."

Suggested fix: either compile imgui_impl_osx.mm on macOS in the Metal cc::Build and declare the three externs under cfg(target_os="macos"), or record the omission as an accepted deviation in the port ledger.

#### S31-F4 — 10 of 16 DockBuilder extern declarations missing

- **Status: DISPUTED** (refuted by the **semantic** lens)
- Kind: missing-function · Severity: low
- Oracle: `pkg/dcimgui/main.zig:52-67`
- Rustty: `crates-sys/dcimgui-sys/src/lib.rs:110-148` (deferral comment at :91-107)

The reference's manual DockBuilder extern block declares 16 functions. rustty declares only SplitNode, DockWindow, Finish, GetNode, AddNodeEx, SetNodeSize. Missing: GetCentralNode, AddNode, RemoveNode, RemoveNodeDockedWindows, RemoveNodeDockedWindowsEx, RemoveNodeChildNodes, SetNodePos, CopyDockSpace, CopyNode, CopyWindowSettings. None is called anywhere in ghostty src/.

Refuting lens — **semantic**: "main.zig:52-73 declares 16 DockBuilder externs; lib.rs:110-149 declares 6 (SplitNode, DockWindow, Finish, GetNode, AddNodeEx, SetNodeSize). Grepping the oracle src/ for DockBuilder (inspector/widgets.zig:68-76, widgets/surface.zig:160-184, widgets/screen.zig:134-147) shows exactly those 6 are ever called; the other 10 are declaration-only in the oracle. Missing unused extern declarations in a bindings shim change no runtime behaviour at any use-site, and lib.rs:93-95 documents the deliberate narrowing."

Suggested fix: declare the remaining 10 externs (all exported by the compiled dcimgui_internal.cpp) to match the reference surface.

#### S31-F5 — ImGuiDockNodeFlagsPrivate: only DockSpace (1<<10) ported; 12 sibling constants missing

- **Status: DISPUTED** (refuted by the **semantic** lens)
- Kind: different-constant · Severity: low
- Oracle: `pkg/dcimgui/main.zig:37-51`
- Rustty: `crates-sys/dcimgui-sys/src/lib.rs:164-172`

The reference defines 13 private dock-node flag constants (DockSpace 1<<10 through NoDockingOverEmpty 1<<22). rustty exposes only `IM_GUI_DOCK_NODE_FLAGS_PRIVATE_DOCK_SPACE = 1 << 10` (value matches). Only DockSpace is used by ghostty src/ (widgets/surface.zig:165, widgets/screen.zig:140), so no use-site differs.

Refuting lens — **semantic**: "main.zig:37-51 defines 13 ImGuiDockNodeFlagsPrivate constants; lib.rs:168-170 defines only IM_GUI_DOCK_NODE_FLAGS_PRIVATE_DOCK_SPACE = 1 << 10, and that value matches the oracle's DockSpace = 1 << 10 exactly. Oracle grep shows only ImGuiDockNodeFlagsPrivate.DockSpace is referenced (widgets/surface.zig:165, widgets/screen.zig:140). Unused constant declarations produce no observable difference."

Suggested fix: add the 12 remaining constants with the reference's bit positions.

#### S33-F2 — harfbuzz-sys lib.rs/Cargo.toml doc comments claim CoreText is off, contradicting build.rs/wrapper.h

- **Status: DISPUTED** (refuted by the **semantic** lens)
- Kind: other · Severity: low
- Oracle: `pkg/harfbuzz/build.zig:168-171`
- Rustty: `crates-sys/harfbuzz-sys/src/lib.rs:10-11` (also Cargo.toml:11-12, wrapper.h:4-7)

lib.rs:10-11 and the Cargo.toml header say "CoreText is off so hb-coretext.h is omitted" / "CoreText is OFF for the vendored build", but build.rs:88-91 defines HAVE_CORETEXT=1 on macOS and wrapper.h:18-20 includes hb-coretext.h under `__APPLE__` (and hb.rs relies on hb_coretext_font_create). Doc-only inconsistency; the compiled behaviour matches the oracle's `enable-coretext = hasCoretext()`.

Refuting lens — **semantic**: "The stale wording is real (crates-sys/harfbuzz-sys/src/lib.rs:10-11 says 'CoreText is off so hb-coretext.h is omitted' while build.rs:88-91 defines HAVE_CORETEXT=1 on macOS and wrapper.h:14-20 includes hb-coretext.h under **APPLE**), but the finder itself concedes the compiled behaviour matches the oracle's `enable-coretext = hasCoretext()`. This is an internal doc-comment inaccuracy in rustty, not a divergence from the oracle: no input or state produces a different result, error, or side effect. Under the semantic-equivalence lens it is refuted (and it is also outside the shard's pkg/gtk4-layer-shell area)."

Suggested fix: update the lib.rs and Cargo.toml comments to state CoreText is enabled on macOS.

#### S34-F7 — LoongArch target detection (lsx/lasx) has no counterpart; loongarch falls to `return 0`

- **Status: DISPUTED** (refuted by the **semantic** lens; severity corrected high -> low)
- Kind: missing-file · Severity: low
- Oracle: `pkg/highway/src/detect/loongarch.zig:1-26`
- Rustty: `crates-sys/highway-sys/cpp/detect.c:188-190`

detect.zig:30 routes .loongarch32/.loongarch64 to loongarch.detect(): on Linux sets lsx when AT_HWCAP has HWCAP_LSX(1<<4), and lasx additionally when HWCAP_LASX(1<<5). detect.c has no loongarch branch and returns 0; no TGT_LSX/TGT_LASX defines exist.

Refuting lens — **semantic**: "The oracle's loongarch.detect() is unreachable in this package's actual build. Its only caller is pkg/highway/src/cpp/targets.cpp:28-38, whose call is gated `#if (…|| HWY_ARCH_LOONGARCH) && HWY_HAVE_RUNTIME_DISPATCH`; in the vendored Highway (66486a1) detect_targets.h:760-778, HWY_HAVE_RUNTIME_DISPATCH_LINUX is 1 only for `(HWY_ARCH_ARM || HWY_ARCH_PPC || HWY_ARCH_S390X) && HWY_OS_LINUX`, and HWY_HAVE_RUNTIME_DISPATCH is otherwise 1 only for x86 / RVV(hard-disabled by `&& 0` at :744) / Apple ARM. LoongArch is therefore never a runtime-dispatch arch, so `ghostty_hwy_detect_targets` is not called there and DetectTargets takes the `bits |= HWY_ENABLED_BASELINE` arm identically in both trees (rustty's cpp/targets.cpp is byte-identical to the oracle's apart from the symbol rename — verified by diff). A grep of the whole ghostty tree finds no other caller of ghostty_hwy_detect_targets (only detect.zig:23 and targets.cpp:6/24/34). Even if it were called, an unset LSX/LASX bit only selects EMU128/SCALAR kernels with identical numeric results (perf-only). NOTE for the parent, out of scope of this id: rustty/crates-sys/highway-sys/cpp/detect.c has no **powerpc** or **s390x** branch either (oracle detect.zig:26-27 -> ppc.zig/s390x.zig), and those ARE reachable (HWY_HAVE_RUNTIME_DISPATCH_LINUX covers PPC/S390X on Linux) — that is the defensible version of this class of gap."

Suggested fix: add an `#elif defined(__loongarch__)` branch porting loongarch.zig with TGT_LASX (1<<40), TGT_LSX (1<<41).

#### S35-F1 — pkg/macos foundation wrapper tests (16 test blocks) have no rustty counterpart

- **Status: DISPUTED** (refuted by the **presence** lens; the semantic lens narrowed it to the one ported-without-test helper)
- Kind: missing-test · Severity: low
- Oracle: `pkg/macos/foundation/array.zig:123-176` plus attributed_string.zig:89-103, character_set.zig:31-49, data.zig:32-38, dictionary.zig:92-125, number.zig:69-79, string.zig:118-120,149-174, url.zig:68-104
- Rustty: `crates/rustty-font-shaper/src/shaper/coretext.rs:1272-1285` (surrogate-pair helper, no #[test]); the other 15 oracle test blocks are not-applicable

The oracle's foundation binding layer carries 16 test blocks with concrete assertions (array/array sorting, mutable attributed string, character set, data, dictionary/mutable dictionary, number, string, unichar, StringComparison bit size, url, path). Rustty replaced the whole wrapper with objc2-core-foundation, so the wrappers do not exist and none of these tests were carried over. The one durable nub: the CF_INLINE `stringGetSurrogatePairForLongCharacter` helper _was_ hand-ported into production Rust in four places (deferred_face.rs:192, face/coretext.rs:94, shaper/coretext.rs:1273, discovery.rs:1025) and carries no test, while the oracle's test "unichar" asserts `!stringGetSurrogatePairForLongCharacter('A')`.

Refuting lens — **presence**: "The oracle tests being cited (array.zig:123-176, string.zig:118-120/149-174, url.zig:68-104, etc.) exercise ghostty's own hand-written CoreFoundation wrapper types (String, Array, Dictionary, Number, URL). Per the shard ground rules, pkg/* Apple binding layers are legitimately REPLACED (rustty uses objc2-core-foundation: see crates/rustty-font-shaper/src/discovery.rs:862-871 importing CFArray/CFDictionary/CFNumber/CFString/CFCharacterSet/CFRange directly), so the wrapper types have no in-tree counterpart and tests of a third-party crate's own API are not required parity artifacts. The only genuinely in-tree re-implementation traced from these files is string_get_surrogate_pair_for_long_character (crates/rustty-font-shaper/src/discovery.rs:1025-1038, plus copies in deferred_face.rs/face/coretext.rs/shaper/coretext.rs), which is a 6-line pure-Rust decomposition with no test; the oracle's counterpart test 'unichar' (string.zig:170-175) has a single assertion. That single low-value smoke assertion against a replaced binding layer does not clear the bar for a defensible finding, and the finding itself concedes it is 'arguably not-applicable'. Correct disposition: status 'replaced'/'not-applicable' in the ledger, not a finding."

Suggested fix: either record these as not-applicable in the parity ledger, or add one macOS-gated #[test] covering the surrogate-pair helper ('A' -> false, U+1F600 -> D83D/DE00) plus a CFDictionary/CFNumber round-trip.

#### S36-F4 — Log::create does not check os_log_create for NULL (reference unwraps with `.?`)

- **Status: DISPUTED** (refuted by the **semantic** lens)
- Kind: missing-error-path · Severity: low
- Oracle: `pkg/macos/os/log.zig:7-15`
- Rustty: `crates/rustty-os/src/macos.rs:297-302`

The reference `Log.create` applies `.?` to the os_log_create result, so a NULL handle is a checked panic at creation time. rustty stores whatever pointer comes back (the SAFETY comment asserts it "never returns NULL") and would carry a null `os_log_t` into later os_log_type_enabled/os_log_with_type calls. Practically unreachable, but it is a dropped check.

Refuting lens — **semantic**: "Read pkg/macos/os/log.zig:7-15 (`return @ptrCast(c.os_log_create(...).?);`) and crates/rustty-os/src/macos.rs:296-302 (stores the pointer, SAFETY comment noting os_log_create never returns NULL). os_log_create is documented never to return NULL (it returns OS_LOG_DEFAULT/aborts internally), and in ReleaseFast the Zig `.?` is itself unreachable/UB rather than a checked error. The difference is therefore confined to unreachable code with no input producing a different observable result — refuted per the lens."

Suggested fix: add `assert!(!p.is_null())` (or `.expect`) on the os_log_create result in Log::create.

#### S37-F1 — Program.setUniform not ported (7-arm glUniform* dispatch + UniformNameInvalid error path)

- **Status: DISPUTED** (refuted by the **semantic** lens; severity corrected medium -> low)
- Kind: missing-function · Severity: low
- Oracle: `pkg/opengl/Program.zig:90-125`
- Rustty: `crates/rustty-renderer/src/opengl/gl/program.rs:12-28, 74-80`

The reference exposes `setUniform(p, n, value: anytype)` which calls glGetUniformLocation, returns `error.UniformNameInvalid` when loc < 0, calls errors.getError(), dispatches on the value type to Uniform1i/1f/2f/3f/4f/UniformMatrix4fv, then getError() again. rustty declares the `UniformNameInvalid` variant but has no set_uniform method at all; the module doc records this as a deliberate omission because the reference has zero call sites.

Refuting lens — **semantic**: "Program.zig:90-125 setUniform is dead code in the oracle: `grep -rn setUniform` over /Users/gael/projects/ghostty/src and /pkg returns only its own definition (Program.zig:91). pkg/opengl is an in-tree package with no external consumers, and the live uniform path is the UBO one (RenderPass.zig bindBase, ported in buffer.rs). No input or state can reach the missing dispatch, so no observable behaviour differs; program.rs:12-32 documents the omission and its recoverability. Difference confined to unreachable code -> refuted per the lens."

Suggested fix: add `pub fn set_uniform<V: UniformValue>(self, n: &CStr, value: V) -> Result<(), Error>` implementing GetUniformLocation -> loc<0 => Err(UniformNameInvalid) -> get_error()? -> the matching glUniform* call -> get_error().

#### S37-F2 — macos.video.PixelFormat (CVPixelFormatType enum, 83 members) has no counterpart

- **Status: DISPUTED** (refuted by the **semantic** lens)
- Kind: missing-file · Severity: low
- Oracle: `pkg/macos/video/pixel_format.zig:1-171`
- Rustty: absent; nearest sibling port is `crates/rustty-renderer/src/metal/cvdisplaylink.rs:1-87`

pkg/macos/video.zig re-exports `PixelFormat = enum(c_int)` binding every kCVPixelFormatType_* constant with a non-exhaustive tail. Nothing in the rustty workspace binds these constants (grep hits only a comment in metal/target.rs:102). The oracle itself has no use-site of video.PixelFormat outside the re-export.

Refuting lens — **semantic**: "pkg/macos/video.zig only re-exports PixelFormat; grep for PixelFormat across ghostty/src and ghostty/macos finds no use of macos.video.PixelFormat (only unrelated renderer/Metal enums). It is an unused constant table in a pkg/* Apple binding layer, which the ground rules treat as replaced by crates-sys/binding crates; every use-site behaviour the oracle relies on (CVDisplayLink create/start/stop/isRunning/setCurrentCGDisplay/setOutputCallback) is present in cvdisplaylink.rs:71-87 and below. No observable behaviour depends on the enum."

Suggested fix: if full pkg/macos surface parity is wanted, add a `PixelFormat(pub i32)` newtype with the 83 associated constants next to cvdisplaylink.rs; otherwise record it as an intentionally-unbound, unused binding.

#### S38-F1 — `too_big` fixtures exist in the oracle checkout but the port claims they are absent

- **Status: DISPUTED** (refuted by the **semantic** lens)
- Kind: other · Severity: low
- Oracle: `pkg/wuffs/src/png.zig:148-151` (and jpeg.zig:148-151)
- Rustty: `crates/rustty-wuffs/src/test_fixtures.rs:3-8` (claim), `png.rs:245-261`, `jpeg.rs:236-253`

The reference tests `png: too big` / `jpeg: too big` @embedFile `too_big.png` / `too_big.jpg`, which ARE present in the pinned checkout at pkg/wuffs/src/. The port's PORT NOTEs state they are ABSENT (F5) and instead decode a hand-built PNG (IHDR 32769x32768 + token IDAT) / a SOF0-patched JPEG. The assertion (`Err(Error::Overflow)`) is the same and the declared dimensions match the reference blobs, so the test's discriminating power is preserved; only the input and the provenance claim differ. (The `1x1#*.{png,jpg}` fixtures come from the lazy `pixels` dependency and are genuinely not in the checkout.)

Refuting lens — **semantic**: "Verified both sides: `ls /Users/gael/projects/ghostty/pkg/wuffs/src` does list too_big.png/too_big.jpg, so the PORT NOTE wording in crates/rustty-wuffs/src/test_fixtures.rs:3-4 (\"ABSENT from the ghostty checkout\") is factually wrong for those two blobs. But the test behaviour is equivalent: png.zig:148-151 / jpeg.zig:148-151 assert only `expectError(error.Overflow)`, and png.rs:256-261 / jpeg.rs:250-254 assert `Err(Error::Overflow)` on inputs whose IHDR/SOF0 declare the same 32769x32768 dimensions, hitting the same `> maximum_image_size` guard in decode_image_config. Assertions are equal in meaning and not looser, so this is a comment/provenance inaccuracy only (doc-only), not an observable functional difference."

Suggested fix: either check in the reference blobs and decode them in the `too_big` tests, or correct the PORT NOTEs to say the blobs are present but intentionally replaced by minimal fixtures with identical declared dimensions.

#### S38-F6 — No system-integration (`-fsys=spirv-cross`) path; always vendored

- **Status: DISPUTED** (refuted by the **presence** lens)
- Kind: missing-branch · Severity: low
- Oracle: `pkg/spirv-cross/build.zig:34-43`
- Rustty: `crates-sys/spirv-cross-sys/build.rs:57-98`

The reference can link the system `spirv-cross-c-shared` dynamically when `systemIntegrationOption("spirv-cross")` is set (used by distro packaging). spirv-cross-sys always compiles the vendored sources; no env/feature selects a system library. Observable only as a packaging choice, never as different shader output for the same library version.

Refuting lens — **presence**: "The absence is real — I grepped crates/, crates-sys/ and xtask/ for 'spirv-cross-c-shared', 'systemIntegration' and 'system_integration' and got zero hits — but this is the replaced-build-plumbing category: `b.systemIntegrationOption` is a zig-build packaging switch with no Cargo analogue, and the finding itself concedes it can 'never' change observable behaviour for the same library version (build.zig:34-42 links the SAME spirv-cross C API either way, and spirv-cross-sys:57-98 compiles the vendored sources to provide exactly those symbols). Every use-site behaviour the oracle relied on (the spirv_cross_* C entry points) is present, so there is no observable functional difference to report."

Suggested fix: optionally add a `RUSTTY_SYS_SPIRV_CROSS=system` (or Cargo feature) branch that emits `cargo:rustc-link-lib=dylib=spirv-cross-c-shared` and skips the cc build.

### Unverified

None — every finding in this area was verified by both lenses; nothing exceeded the verification cap.

## Performance-only differences (not findings)

- `fontconfig-sys/build.rs` pins `-std=gnu17` which the reference gets implicitly from zig's clang default; language-mode only.
- `dcimgui-sys` compiles with `.std("c++17")` and passes `-w` to silence vendored warnings; the reference uses the toolchain default gnu++17 with warnings enabled. Diagnostics-only.
- `dcimgui-sys` bindgen additionally defines IMGUI_DISABLE_OBSOLETE_FUNCTIONS when generating bindings; this only trims declarations for functions not compiled into the archive anyway.
- `fc.rs` adds a test-only RwLock (FC_GLOBAL) so `fini()` cannot race sibling tests under cargo's parallel harness; zig test runs serially. Test-scheduling only.
- `fc.rs` `Property::cval` uses static C-string literals instead of the reference's comptime underscore-stripping loop; all 50 produced strings checked and matching.
- `ft.rs` `LoadFlags` is a plain struct with explicit `to_bits`/`from_bits` rather than a Zig packed struct; bit layout verified identical.
- `ft.rs` `load_sfnt_table` zero-initialises the Vec where the Zig allocates uninitialised memory; FreeType overwrites the whole buffer.
- `shadertoy.rs` serialises glslang/spirv-cross FFI behind a process-global Mutex because cargo test runs in parallel; identical results.
- Build-only differences in replaced build.zig units: freetype-sys always vendors (no system-integration/dynamic-link option), omits the Windows ftver.rc resource, always enables libpng; glslang-sys adds `-include cstdint` for newer libstdc++ and silences vendored warnings.
- `hb.rs` `Blob::set_user_data` transmutes the typed destroy callback to the void* signature instead of generating a comptime trampoline (identical C ABI).
- `hb.rs` Script/ContentType/Direction use explicit to_c/from_c match tables instead of integer-backed Zig enums; unknown tags decode to the Invalid sentinel where the oracle's `@enumFromInt` is illegal behaviour.
- `Font::set_scale` uses `as i32` (wrapping) where the oracle's `@intCast` is safety-checked; differs only for values >i32::MAX, illegal behaviour in the reference.
- `harfbuzz-sys` always compiles hb-ft (HAVE_FREETYPE=1) even on the CoreText backend; extra code compiled in, no observable difference.
- Highway/gettext/libpng/libxml2 `-sys` builds pass `.warnings(false)`/`-w` in place of the oracle's diagnostic-only `-W*` flags; warnings never change codegen.
- `pkg/highway/src/detect/linux.zig` avoids libc (direct syscalls) for binary size; detect.c uses libc getauxval()/prctl(). Same values.
- `highway-sys/build.rs` adds `-std=c++17` explicitly and compiles detect.c in a second (C) cc::Build — organisation only.
- `libxml2-sys/build.rs` pins `-std=gnu17`; `libpng-sys` adds `-fPIC` on freebsd/musl and `libxml2-sys` adds `-liconv` on Apple — link/front-end only.
- `ghostty_hwy_detect_targets` is renamed `rustty_hwy_detect_targets` consistently in both targets.cpp and detect.c; not part of any public header.
- `dispatch/data.zig` `Data.create` passes the main dispatch queue; `DispatchData::from_bytes` passes null. With DISPATCH_DATA_DESTRUCTOR_DEFAULT the buffer is copied and the queue only runs a destructor block.
- `foundation/string.zig` `cstringPtr` (zero-copy, may return NULL) is replaced at its DeferredFace/face `name()` use sites by a CFString Display -> copy into the caller's buffer; identical bytes, one extra copy.
- `run.zig` `get*Ptr` returns a static empty slice for len==0; rustty allocates a zero-length Vec — identical observable result.
- `signpost.zig`'s module-private `__dso_handle` is a plain global; rustty uses an AtomicPtr with Acquire/Release — same idempotent init semantics; init resolves the DSO from its own address instead of the root module's `main` (same image, same dli_fbase).
- `rustty-os` wraps os_log/os_signpost FFI in `cfg(miri)` no-op shims so pure-Rust callers stay in the Miri lane.
- `Log.log` takes a pre-formatted `&str` instead of `allocPrintSentinel(format, args)`; the same string reaches os_log_with_type via the same `%{public}s` shim.
- `rustty-oniguruma` `testing::ensure_init` uses a OnceLock so onig_initialize runs exactly once (thread-safe); the reference's flag is never written so it re-runs each call under serial zig test.
- `oniguruma-sys/build.rs` additionally compiles `vendor/src/koi8.c` so the `Encoding::koi8()` accessor resolves at link time (the reference exposes it but never links it, Zig being lazy). Extra capability.
- `pkg/opengl/glad.zig`'s threadlocal GladGLContext is replaced by the `gl` crate's process-global entry-point statics; version parsed from `glGetString(GL_VERSION)`; `glad.unload()` has no analogue (only use is an errdefer).
- `rustty-oniguruma` `Regex::search_with_param` takes a required `&mut MatchParam` (frozen vt API), the optional form kept as the private `search_impl`; both reference code paths remain reachable.
- `pkg/opengl` open (`_`-tailed) enums are modelled as repr-transparent newtypes with associated constants so `@enumFromInt` of arbitrary driver values stays sound; closed enums are real Rust enums, and rustty adds constant-identity tests the Zig compiler provides for free.
- `wuffs-sys` compiles a second non-WUFFS_IMPLEMENTATION shim TU exposing `rustty_wuffs_*` wrappers over `static inline` helpers bindgen cannot bind; identical results.
- `rustty-wuffs` backs the opaque decoder struct with an 8-aligned `Scratch` allocation and hands the decoded buffer out as an align-1 `RawBuf` owning the same allocation; same bytes.
- `sentry-sys` compiles `sentry_backend_breakpad.cpp` in a separate C++ cc sub-build archived into the same libsentry.a; link result identical.
- `crates-sys/zlib-sys/build.rs:43-62` lists deflate.c/trees.c/compress.c twice; `ar r` replaces the member, so the archive is unaffected (build-time waste only).
- `zlib-sys` omits `-DHAVE_SYS_TYPES_H/-DHAVE_STDINT_H/-DHAVE_STDDEF_H` (unreferenced by any vendored 1.3.1 source) and `-DZ_HAVE_UNISTD_H` (affects only z_off_t width, identical on LP64, and the un-vendored gz* units).
- `Command.start`: the oracle's ArenaAllocator collapses into owned Vec temporaries all created before `fork()`; identical allocation ordering contract.
- `App`: `?std.Io.Timestamp` -> `Option<Instant>`; `Mailbox` gains a rustty-only `push_or_return` retry primitive with the same unconditional wakeup; `Message` boxed-by-value in a `BlockingQueue<_, 64>` exactly as the reference.
- `Command.wait` Windows path binds WaitForSingleObject/GetExitCodeProcess locally and opens "NUL" via CreateFileW instead of NtCreateFile with the same flags (structural, unverified on Windows per DEFERRALS.md).

## Refuted candidates

Checked and cleared by both verification lenses — listed so a reader can see what was examined:

- **S31-F6 — fc_match test omits the reference's per-font destroy loop.** All 6 assertions are present with equal strength; the omitted `defer for (fs.fonts()) |font| font.destroy()` is a documented double-free fix (ledger P4-U3), teardown-only.
- **S32-F1 — pkg/glslang shader.zig test (simple.frag) has no counterpart.** `simple.frag` is a snapshot of the shadertoy prefix; `shadertoy.rs` test `spirv` drives the same 13-field glslang_input_t over `shadertoy_prefix.glsl` and asserts create+preprocess+parse plus link and SPIR-V generation — strictly more than the oracle's two asserts.
- **S32-F2 — pkg/glslang wrapper surface (finalize, spirvGet, spirvGetMessages, opaque Shader/Program) not ported.** None is called anywhere in the oracle src tree; the raw C entry points are all present in glslang-sys (the KAT even calls two of them). Wrapper-shape only.
- **S34-F8 — No typed `Targets` bit-struct / `supported_targets()` wrapper; only 8 of 29 bit positions reproduced.** The runtime mask is identical; the oracle wrapper is a pure `@bitCast` with no consumer, its comptime asserts are compile-time self-checks, and rustty's test asserts the mask is non-zero (stronger than the oracle's `_ = supported_targets()`).
- **S34-F9 — MSVC-only Highway compile flags not mirrored.** The oracle arm targets `zig cc` (clang) on the msvc ABI; rustty's msvc lane is check-only (build.rs returns before any cc invocation) and `cc` would drive cl.exe, which rejects those `-f*` flags. All behaviour-affecting flags are mirrored.
- **S36-F2 — IOSurface.deinit's IOSurfaceSetPurgeable(kIOSurfacePurgeableEmpty) dropped.** The surface is released on the same path either way; the purgeable call only makes backing pages reclaimable sooner — memory-release timing, i.e. performance-only.
- **S36-F6 — Binding-layer smoke/layout tests of the replaced pkg/macos wrappers have no direct counterparts.** They test the wrapper layer rustty legitimately replaces with objc2; the packed-struct bitSize/bitcast half is not applicable, and the Monaco/CGBitmapContext/CTLine paths are exercised by face/coretext.rs `render_ascii`, discovery.rs and shaper/coretext.rs tests.
- **S37-F3 — Buffer.Binding.attribute not ported.** The oracle body calls `b.enableAttribArray`, which `Buffer.Binding` does not have, uses the pre-0.14 `.Array` typeInfo tag, and has no call site — it has never been compiled; its three real helpers are ported.
- **S37-F4 — dataInfo `.one` (single-item pointer) arm not ported.** Every live oracle caller passes a slice; rustty reproduces the reached `.slice` arm exactly and the two `unreachable` arms become compile-time rejections of the same contract.
- **S37-F6 — set_retry_limit_in_search uses a truncating `as c_ulong`.** On every target either tree builds (LP64) `usize` and `c_ulong` are the same width, so the conversion is the identity; the Zig `@intCast` check is elided in ReleaseFast anyway.
- **S38-F3 — sentry_pkg string parameters narrowed from `[]const u8` to `&str`.** Every function is present with the same `_n` C call and ptr/len pair; all callers in both trees pass ASCII literals/tag names and `writeToFile` has no caller. Type-surface only.
- **S38-F7 — maximum_image_size typed as usize does not compile on 32-bit targets.** Same value, same `>` guard, same warn-then-Overflow path on every built (64-bit) target; a hypothetical 32-bit compile concern on a lane nothing builds, and the constant's value is not actually different.
- **S39-F3 — PATH read from live libc getenv instead of the captured global environ.** The oracle's `global.environ()` is a fat pointer over the LIVE `std.c.environ` block (re-synced on every mutation), so both read the same value, with byte-identical fallbacks; no PATH setenv exists anywhere in rustty.
