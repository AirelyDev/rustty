# Parity closure checklist — pkg

33 open finding(s): 4 high, 7 medium, 22 low. Full evidence per row is in [`docs/parity-audit/pkg.md`](../../docs/parity-audit/pkg.md).

A row is CLOSED only when its fix has a test that fails without it (or a written, reviewed justification for why no test is possible).


## High (4)

- [x] **S34-F1** · CONFIRMED · `INDIV` · behavioral-difference
  - **What:** x86 AVX2 gate drops the XCR0 (xgetbv) OS-support check and the LZCNT/BMI/BMI2 requirements
  - **Oracle:** `pkg/highway/src/detect/x86.zig:63-94` → **Rustty:** `crates-sys/highway-sys/cpp/detect.c:172-183`
  - **Audit's suggested fix:** Port x86.zig:63-94 literally: read XCR0 with `_xgetbv(0)` (or inline asm) when OSXSAVE&&AVX, require (xcr0&0x6)==0x6, then require leaf 0x80000001 ecx bit5 and leaf7 ebx bits 3 and 8 before setting TGT_AVX2; gate the whole block on has_avx_save && max_leaf>=7 as the oracle does.
- [ ] **S34-F4** · CONFIRMED · `INDIV` · missing-file
  - **What:** PowerPC target detection (ppc8/ppc9/ppc10) has no counterpart; powerpc falls to `return 0`
  - **Oracle:** `pkg/highway/src/detect/ppc.zig:1-43` → **Rustty:** `crates-sys/highway-sys/cpp/detect.c:188-190`
  - **Audit's suggested fix:** Add an `#elif defined(__powerpc__) || defined(__powerpc64__)` branch to detect.c porting ppc.zig with TGT_PPC10 (1<<47), TGT_PPC9 (1<<48), TGT_PPC8 (1<<49), gated on __linux__.
- [ ] **S34-F5** · CONFIRMED · `INDIV` · missing-file
  - **What:** s390x target detection (z14/z15) has no counterpart; s390x falls to `return 0`
  - **Oracle:** `pkg/highway/src/detect/s390x.zig:1-29` → **Rustty:** `crates-sys/highway-sys/cpp/detect.c:188-190`
  - **Audit's suggested fix:** Add `#elif defined(__s390x__)` branch porting s390x.zig with TGT_Z15 (1<<50), TGT_Z14 (1<<51).
- [ ] **S34-F6** · CONFIRMED · `INDIV` · missing-file
  - **What:** RISC-V target detection (rvv) has no counterpart; riscv falls to `return 0`
  - **Oracle:** `pkg/highway/src/detect/riscv.zig:1-22` → **Rustty:** `crates-sys/highway-sys/cpp/detect.c:188-190`
  - **Audit's suggested fix:** Add `#elif defined(__riscv)` branch porting riscv.zig with TGT_RVV (1<<37).

## Medium (7)

- [ ] **S31-F1** · CONFIRMED · `INDIV` · missing-branch
  - **What:** FreeBSD fontconfig directory defines not ported (Linux paths used on FreeBSD)
  - **Oracle:** `pkg/fontconfig/build.zig:178-190` → **Rustty:** `crates-sys/fontconfig-sys/build.rs:210-217`
  - **Audit's suggested fix:** Add `if target_os == "freebsd" { define FC_TEMPLATEDIR=/usr/local/etc/fonts/conf.avail, FONTCONFIG_PATH=/usr/local/etc/fonts, CONFIGDIR=/usr/local/etc/fonts/conf.d } else { existing }` in the non-Windows arm.
- [ ] **S34-F2** · CONFIRMED · `INDIV` · missing-branch
  - **What:** x86 AVX-512 (avx3/avx3_dl/avx3_zen4/avx3_spr) and AVX10 detection, plus isAMD(), are not ported
  - **Oracle:** `pkg/highway/src/detect/x86.zig:72-76, 96-154, 160-166` → **Rustty:** `crates-sys/highway-sys/cpp/detect.c:172-186`
  - **Audit's suggested fix:** Port the AVX-512 and AVX10 blocks and isAMD() from x86.zig verbatim into detect.c, adding TGT_AVX3_DL (1<<7), TGT_AVX3_ZEN4 (1<<6), TGT_AVX3_SPR (1<<4) and the has_avx512_save gate (#ifdef __APPLE__ true, else (xcr0 & 0xE0) == 0xE0).
- [ ] **S34-F3** · CONFIRMED · `INDIV` · missing-branch
  - **What:** aarch64 Linux never sets sve / sve_256 / sve2 / sve2_128; SVE vector length is probed and discarded
  - **Oracle:** `pkg/highway/src/detect/aarch64_linux.zig:19-20, 33-50` → **Rustty:** `crates-sys/highway-sys/cpp/detect.c:111-115`
  - **Audit's suggested fix:** Add TGT_SVE2_128 (1<<18), TGT_SVE_256 (1<<19), TGT_SVE2 (1<<23), TGT_SVE (1<<24), HWCAP2_SVE2_ (1UL<<1), HWCAP2_SVEAES_ (1UL<<2) and port the block literally, using the sve_vector_bytes() result.
- [ ] **S36-F1** · CONFIRMED · `INDIV` · stub-or-unimplemented
  - **What:** IOSurface Display P3 color space (createNamed(.displayP3) + setColorSpace) is a deferred seam — never applied to the render target
  - **Oracle:** `pkg/macos/iosurface/iosurface.zig:19,47-49,73-84 (with pkg/macos/graphics/color_space.zig:21-26 and use-site src/renderer/metal/Target.zig:51-60)` → **Rustty:** `crates/rustty-renderer/src/metal/target.rs:80-98,145`
  - **Audit's suggested fix:** In Target::init, after IOSurfaceRef::new: create CGColorSpace::with_name(kCGColorSpaceDisplayP3) (objc2-core-graphics is already a workspace dep via rustty-font-shaper), call CGColorSpaceCopyPropertyList on it and IOSurfaceSetValue(surface, kIOSurfaceColorSpace, plist), releasing the plist afterwards — i.e. port iosurface.zig:73-84 as an IOSurface helper in target.rs.
- [ ] **S36-F3** · CONFIRMED · `ALLOC` · missing-error-path
  - **What:** NULL results from CTFont/CTFontDescriptor/CTFontCollection creators become objc2 panics instead of the reference's error.OutOfMemory / `catch return null`
  - **Oracle:** `pkg/macos/text/font.zig:10-19,44-59 (also font_descriptor.zig:7-47, font_collection.zig:15-23, use-site src/apprt/embedded.zig:2434-2438)` → **Rustty:** `crates/rustty-font-shaper/src/face/coretext.rs:212,227,354,363,428,444,456,1364 (also discovery.rs:1156,1225,1654,2057,2065; shaper/coretext.rs:1063,1071; rustty-apprt-c/src/lib.rs:2921-2925)`
  - **Audit's suggested fix:** Declare the raw CTFontCreateWithFontDescriptor / CTFontCreateCopyWithAttributes / CTFontDescriptorCreateWithAttributes / CTFontDescriptorCreateCopyWithAttributes / CTFontDescriptorCreateCopyWithVariation / CTFontCollectionCreateWithFontDescriptors symbols in the existing `sys` modules (same pattern as CTFontCreateWithName) returning `*mut T`, and map NULL to Error::OutOfMemory (or `return null` in the apprt-c export).
- [ ] **S36-F5** · CONFIRMED · `INDIV` · missing-test
  - **What:** No test for the 'from descriptors no match' branch (NULL matching-descriptor array -> empty list)
  - **Oracle:** `pkg/macos/text/font_collection.zig:117-139` → **Rustty:** `crates/rustty-font-shaper/src/discovery.rs:1520-1547 (tests module 2590-2900)`
  - **Audit's suggested fix:** Add a discovery test that calls CoreText::discover with family 'ThisShouldNeverExist' and asserts the iterator yields nothing (list.len() == 0).
- [ ] **S39-F1** · CONFIRMED · `INDIV` · behavioral-difference
  - **What:** PATH search: EPERM from execve continues the search in rustty but aborts it in the oracle
  - **Oracle:** `src/Command.zig:226-256` → **Rustty:** `crates/rustty-termio/src/command.rs:1518-1650`
  - **Audit's suggested fix:** Have execvpeZ_search operate on raw errno values like the oracle (or add a distinct `PermissionDenied`/EPERM variant that falls into the `else => break` arm), and carry the raw errno into the report.

## Low (22)

- [ ] **S31-F2** · DISPUTED · `INDIV` · missing-branch
  - **What:** Windows+libxml2 `-Werror=implicit-function-declaration` flag not mirrored
  - **Oracle:** `pkg/fontconfig/build.zig:223-228` → **Rustty:** `crates-sys/fontconfig-sys/build.rs:227-234`
  - **Audit's suggested fix:** Add `if windows { build.flag_if_supported("-Werror=implicit-function-declaration"); }` after the libxml2 defines for completeness.
- [ ] **S31-F3** · DISPUTED · `INDIV` · missing-function
  - **What:** ImGui_ImplOSX_{Init,Shutdown,NewFrame} externs and imgui_impl_osx.mm backend not present
  - **Oracle:** `pkg/dcimgui/main.zig:29-32` → **Rustty:** `crates-sys/dcimgui-sys/src/lib.rs:154-330`
  - **Audit's suggested fix:** Either compile imgui_impl_osx.mm on macOS in the Metal cc::Build and declare the three externs under cfg(target_os="macos"), or record the omission as an accepted deviation in the port ledger.
- [ ] **S31-F4** · DISPUTED · `INDIV` · missing-function
  - **What:** 10 of 16 DockBuilder extern declarations missing
  - **Oracle:** `pkg/dcimgui/main.zig:52-67` → **Rustty:** `crates-sys/dcimgui-sys/src/lib.rs:97-137`
  - **Audit's suggested fix:** Declare the remaining 10 externs (they are all exported by the compiled dcimgui_internal.cpp) to match the reference surface.
- [ ] **S31-F5** · DISPUTED · `INDIV` · different-constant
  - **What:** ImGuiDockNodeFlagsPrivate: only DockSpace (1<<10) ported; 12 sibling constants missing
  - **Oracle:** `pkg/dcimgui/main.zig:37-51` → **Rustty:** `crates-sys/dcimgui-sys/src/lib.rs:152-153`
  - **Audit's suggested fix:** Add the 12 remaining constants with the reference's bit positions.
- [ ] **S33-F1** · CONFIRMED · `INDIV` · behavioral-difference
  - **What:** setNamespace use site converts the raw byte config value through from_utf8_lossy
  - **Oracle:** `pkg/gtk4-layer-shell/src/main.zig:67-69` → **Rustty:** `crates/rustty-apprt/src/gtk/winproto/wayland.rs:907-909`
  - **Audit's suggested fix:** Call the -sys function (`gtk4_layer_shell_sys::gtk_layer_set_namespace`) with a CString built from the raw bytes, or document that the crate's &str API forces the lossy conversion.
- [ ] **S33-F2** · DISPUTED · `INDIV` · other
  - **What:** harfbuzz-sys lib.rs/Cargo.toml doc comments claim CoreText is off, contradicting build.rs/wrapper.h
  - **Oracle:** `pkg/harfbuzz/build.zig:168-171` → **Rustty:** `crates-sys/harfbuzz-sys/src/lib.rs:10-11`
  - **Audit's suggested fix:** Update the lib.rs and Cargo.toml comments to state CoreText is enabled on macOS.
- [ ] **S34-F7** · DISPUTED · `INDIV` · missing-file
  - **What:** LoongArch target detection (lsx/lasx) has no counterpart; loongarch falls to `return 0`
  - **Oracle:** `pkg/highway/src/detect/loongarch.zig:1-26` → **Rustty:** `crates-sys/highway-sys/cpp/detect.c:188-190`
  - **Audit's suggested fix:** Add `#elif defined(__loongarch__)` branch porting loongarch.zig with TGT_LASX (1<<40), TGT_LSX (1<<41).
- [ ] **S35-F1** · DISPUTED · `ABI` · missing-test
  - **What:** pkg/macos foundation wrapper tests (16 test blocks) have no rustty counterpart
  - **Oracle:** `pkg/macos/foundation/array.zig:123-176 (plus attributed_string.zig:89-103, character_set.zig:31-49, data.zig:32-38, dictionary.zig:92-125, number.zig:69-79, string.zig:118-120,149-174, url.zig:68-104)` → **Rustty:** `crates/rustty-font-shaper/src/discovery.rs:860-870`
  - **Audit's suggested fix:** Either record these as not-applicable in the parity ledger (replaced binding), or add one macOS-gated #[test] in rustty-font-shaper covering the surrogate-pair helper ('A' -> false, U+1F600 -> D83D/DE00) and a CFDictionary/CFNumber round-trip via objc2-core-foundation.
- [ ] **S35-F2** · CONFIRMED · `LOG` · behavioral-difference
  - **What:** Metal error use-site of String.cstringPtr drops the `metal error={s}` log
  - **Oracle:** `pkg/macos/foundation/string.zig:70-77 (use sites src/renderer/metal/shaders.zig:446, src/renderer/metal/Pipeline.zig:202)` → **Rustty:** `crates/rustty-renderer/src/metal/pipeline.rs:181-193 (and shaders.rs:435,445)`
  - **Audit's suggested fix:** In both map_err closures, emit the reference log line (`log::error!("metal error={}", nserr.localizedDescription())`) before returning Error::MetalFailed.
- [ ] **S36-F4** · DISPUTED · `INDIV` · missing-error-path
  - **What:** Log::create does not check os_log_create for NULL (reference unwraps with `.?`)
  - **Oracle:** `pkg/macos/os/log.zig:7-15` → **Rustty:** `crates/rustty-os/src/macos.rs:297-302`
  - **Audit's suggested fix:** Add `assert!(!p.is_null())` (or `.expect`) on the os_log_create result in Log::create.
- [ ] **S37-F1** · DISPUTED · `INDIV` · missing-function
  - **What:** Program.setUniform not ported (7-arm glUniform* dispatch + UniformNameInvalid error path)
  - **Oracle:** `pkg/opengl/Program.zig:90-125` → **Rustty:** `crates/rustty-renderer/src/opengl/gl/program.rs:12-28, 74-80`
  - **Audit's suggested fix:** Add `pub fn set_uniform<V: UniformValue>(self, n: &CStr, value: V) -> Result<(), Error>` (or one method per arm) implementing GetUniformLocation -> loc<0 => Err(UniformNameInvalid) -> get_error()? -> the matching glUniform* call -> get_error().
- [ ] **S37-F2** · DISPUTED · `INDIV` · missing-file
  - **What:** macos.video.PixelFormat (CVPixelFormatType enum, 83 members) has no counterpart
  - **Oracle:** `pkg/macos/video/pixel_format.zig:1-171` → **Rustty:** `crates/rustty-renderer/src/metal/cvdisplaylink.rs:1-87`
  - **Audit's suggested fix:** If full pkg/macos surface parity is wanted, add a `PixelFormat(pub i32)` newtype with the 83 associated constants (values from CVPixelBuffer.h) next to cvdisplaylink.rs; otherwise record it as an intentionally-unbound, unused binding.
- [ ] **S37-F5** · CONFIRMED · `LOG` · behavioral-difference
  - **What:** Monomorphized output_callback drops the reference `log.err("error notifying draw_now err={}")`
  - **Oracle:** `pkg/macos/video/display_link.zig:51-83` → **Rustty:** `crates/rustty-renderer/src/metal/cvdisplaylink.rs:282-288`
  - **Audit's suggested fix:** Emit the error log through the renderer crate's log seam (as generic.rs does elsewhere) inside the Err arm.
- [ ] **S38-F1** · DISPUTED · `TEST` · other
  - **What:** `too_big` fixtures exist in the oracle checkout but the port claims they are absent and substitutes synthetic headers
  - **Oracle:** `pkg/wuffs/src/png.zig:148-151` → **Rustty:** `crates/rustty-wuffs/src/png.rs:245-261`
  - **Audit's suggested fix:** Either check in the reference blobs (or a byte-identical copy) and decode them in the `too_big` tests, or correct the PORT NOTEs to say the blobs are present but intentionally replaced by minimal fixtures with identical declared dimensions.
- [ ] **S38-F2** · CONFIRMED · `LOG` · behavioral-difference
  - **What:** swizzle prepare-failure warning text/scope differs (log text only)
  - **Oracle:** `pkg/wuffs/src/swizzle.zig:172-176` → **Rustty:** `crates/rustty-wuffs/src/swizzle.rs:345-350`
  - **Audit's suggested fix:** Emit the swizzler-scoped message form (`warning(wuffs_swizzler): <msg>`) in the swizzle prepare-failure branch, or give `check` a scope parameter as the reference's `log: anytype` does.
- [ ] **S38-F4** · CONFIRMED · `INDIV` · missing-error-path
  - **What:** Unsupported OS silently builds without a module finder instead of failing the build
  - **Oracle:** `pkg/sentry/build.zig:119-124` → **Rustty:** `crates-sys/sentry-sys/build.rs:164-171`
  - **Audit's suggested fix:** `panic!("target_os={target_os} not supported")` in the fallback arm (freestanding excepted) to mirror the reference's hard error.
- [ ] **S38-F5** · CONFIRMED · `ABI` · missing-branch
  - **What:** simdutf MSVC-specific no_libcxx flags not mirrored
  - **Oracle:** `pkg/simdutf/build.zig:51-72` → **Rustty:** `crates-sys/simdutf-sys/build.rs:47-53`
  - **Audit's suggested fix:** Add `if CARGO_CFG_TARGET_ENV == "msvc" { define _USE_STD_VECTOR_ALGORITHMS=0, _Avx2WmemEnabledWeakValue=_Avx2WmemEnabled; flag -fno-autolink, -fno-stack-protector }`.
- [ ] **S38-F6** · DISPUTED · `INDIV` · missing-branch
  - **What:** No system-integration (`-fsys=spirv-cross`) path; always vendored
  - **Oracle:** `pkg/spirv-cross/build.zig:34-43` → **Rustty:** `crates-sys/spirv-cross-sys/build.rs:57-98`
  - **Audit's suggested fix:** Optionally add a `RUSTTY_SYS_SPIRV_CROSS=system` (or Cargo feature) branch that emits `cargo:rustc-link-lib=dylib=spirv-cross-c-shared` and skips the cc build, mirroring the reference option.
- [ ] **S39-F2** · CONFIRMED · `INDIV` · behavioral-difference
  - **What:** Child exec-failure stderr text prints Zig-std error names instead of the errno tag
  - **Oracle:** `src/Command.zig:258-278` → **Rustty:** `crates/rustty-termio/src/command.rs:713-741, 1479-1516`
  - **Audit's suggested fix:** Format the raw errno as `E<NAME>` (a small errno->name table, or carry the errno and print `E{}` via libc name lookup) to match the oracle text.
- [ ] **S39-F4** · CONFIRMED · `INDIV` · behavioral-difference
  - **What:** setupFd (macOS/FreeBSD/iOS): fcntl/dup2 errno handling differs from the oracle's PosixCall wrapper
  - **Oracle:** `src/Command.zig:483-533` → **Rustty:** `crates/rustty-termio/src/command.rs:1253-1275, 1355-1367`
  - **Audit's suggested fix:** Port the PosixCall wrapper: a local `posix_call(|| ...)` with the oracle's errno switch used by fcntl and dup2 on the Darwin/FreeBSD arm.
- [ ] **S39-F5** · CONFIRMED · `INDIV` · missing-file
  - **What:** zlib-sys omits gzclose/gzlib/gzread/gzwrite/infback translation units the oracle compiles into libz
  - **Oracle:** `pkg/zlib/build.zig:61-77` → **Rustty:** `crates-sys/zlib-sys/build.rs:43-62`
  - **Audit's suggested fix:** Vendor the five remaining verbatim 1.3.1 units and append them to SOURCES (or correct the 'FULL zlib' doc comment to state the subset).
- [ ] **S39-F6** · CONFIRMED · `LOG` · behavioral-difference
  - **What:** All App.zig log.debug/info/warn sites are comment-only in app.rs (no log output)
  - **Oracle:** `src/App.zig:80-81, 94-95, 168, 199, 224, 265-268, 274, 348, 396-399, 484-487, 513-516` → **Rustty:** `crates/rustty-core/src/app.rs:233, 253, 353, 407, 445, 489-495, 519, 590, 640-643, 800-803, 826-829`
  - **Audit's suggested fix:** If/when a logging shim is adopted project-wide, emit these sites; at minimum update the performAllAction comment to the pinned `id={x}` form.
