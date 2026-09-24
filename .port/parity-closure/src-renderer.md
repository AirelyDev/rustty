# Parity closure checklist — src/renderer

14 open finding(s): 2 high, 3 medium, 9 low. Full evidence per row is in [`docs/parity-audit/src-renderer.md`](../../docs/parity-audit/src-renderer.md).

A row is CLOSED only when its fix has a test that fails without it (or a written, reviewed justification for why no test is possible).


## High (2)

- [x] **S117-F1** · CONFIRMED · `ABI` · behavioral-difference
  - **What:** shadertoy::Uniforms drops the per-field align(16); GPU byte layout differs from the std140 block the custom shaders read
  - **Oracle:** `src/renderer/shadertoy.zig:14-42` → **Rustty:** `crates/rustty-renderer/src/shadertoy.rs:63-101`
  - **Audit's suggested fix:** Give the struct the std140 layout explicitly: either wrap the align(16) members in `#[repr(C, align(16))]` newtypes (e.g. `Vec4([f32;4])`, `Vec3([f32;3])` padded) or insert explicit `_pad` fields, then add `const _: () = assert!(offset_of!(Uniforms, channel_time) == 32 && offset_of!(Uniforms, current_cursor) == 208 && offset_of!(Uniforms, palette) == 304 && size_of::<Uniforms>() == 4496)` mirroring gpu_abi.rs; keep the field names so generic.rs's writers are untouched.
- [x] **S118-F1** · CONFIRMED · `INDIV` · missing-branch
  - **What:** IOSurface Display P3 colorspace never set on the render target
  - **Oracle:** `src/renderer/metal/Target.zig:45-60` → **Rustty:** `crates/rustty-renderer/src/metal/target.rs:81-97`
  - **Audit's suggested fix:** Add objc2-core-graphics, create CGColorSpace named Display P3, and set kIOSurfaceColorSpace on the surface (CGColorSpaceCopyPropertyList + IOSurfaceSetValue) in Target::init, matching pkg/macos IOSurface.init's setColorSpace.

## Medium (3)

- [x] **S115-F1** · CONFIRMED · `INDIV` · missing-branch
  - **What:** updateFrame: font_shaper.endFrame() is skipped on the early-return / error paths
  - **Oracle:** `src/renderer/generic.zig:1271-1274, 1319-1322, 1349-1352` → **Rustty:** `crates/rustty-renderer/src/generic.rs:4976-4979, 5018-5019, 5332`
  - **Audit's suggested fix:** Wrap end_frame in a drop guard created at function entry (like the existing `Unlock` guard), or call end_frame explicitly before both early exits.
- [x] **S115-F2** · CONFIRMED · `LOG` · missing-error-path
  - **What:** displayRealized (OpenGL twin) fails and skips the swap-chain/shader rebuild when GL context prep fails; the oracle only logs
  - **Oracle:** `src/renderer/generic.zig:934-959` → **Rustty:** `crates/rustty-renderer/src/generic/linux_impl.rs:1725-1760`
  - **Audit's suggested fix:** Swallow the prepare error (log/ignore) and continue with the lock + initShaders + SwapChain::init sequence, matching the void semantics.
  - **Disposition (task-14c-3):** CLOSED — `generic/linux_impl.rs::display_realized` now calls `self.api.display_realized()` (void) and ALWAYS proceeds to the draw-mutex lock, `init_shaders`, `SwapChain::init` and the flag resets, matching `generic.zig:934-961`. What the code after the warning assumes is documented at the site: the rebuild is now reached with GL possibly not (re-)loaded, which IS the reference's state, and a genuinely dead context fails at the `try` sites (`init_shaders`/`SwapChain::init`) with the leaf GL error instead of being pre-empted by a `GlFailed` that hid it. Test: `generic::linux_impl::tests::linux_opengl_display_realized_rebuilds_even_when_context_prep_fails` asserts the REBUILD (Ok + `swap_chain.is_some()` + `!shaders.defunct` + `!reinitialize_shaders` + `target_config_modified == 1`), not a log line. Negative-controlled twice: once by deleting the emission, once by re-instating the abort while leaving the warning in place. Verified on the VM under `RUSTTY_REQUIRE_GL=1`.
- [ ] **S117-F2** · CONFIRMED · `TEST` · missing-test
  - **What:** OpenGL image twin omits test "kitty renderer ignores pending payloads and removes replaced placements"
  - **Oracle:** `src/renderer/image.zig:1155-1214` → **Rustty:** `crates/rustty-renderer/src/image/linux_impl.rs:2596-2599`
  - **Audit's suggested fix:** Copy macos_impl.rs:2244-2367 verbatim into linux_impl.rs's tests module (it uses only rustty-vt + State/Id/CellSize, so no adaptation is needed).

## Low (9)

- [x] **S114-F1** · CONFIRMED · `INDIV` · missing-branch
  - **What:** cursorBlinkInterval Valgrind x5 branch is inert
  - **Oracle:** `src/renderer/Thread.zig:735-748` → **Rustty:** `crates/rustty-core/src/renderer_seam.rs:1868-1887`
  - **Audit's suggested fix:** Add a `rustty_os::valgrind::running_on_valgrind()` probe (RUNNING_ON_VALGRIND client-request sequence) and re-arm the branch.
- [ ] **S114-F2** · CONFIRMED · `LOG` · missing-error-path
  - **What:** cursorCancelCallback swallows non-Canceled/NotFound cancel errors instead of `unreachable`
  - **Oracle:** `src/renderer/Thread.zig:698-723` → **Rustty:** `crates/rustty-core/src/renderer_seam.rs:1811-1835`
  - **Audit's suggested fix:** Match on the concrete error: Canceled|NotFound => (), other => unreachable!() (cfg'd per backend error set).
- [ ] **S114-F3** · CONFIRMED · `LOG` · other
  - **What:** All std.log calls in the renderer thread are ported as comments (no log output)
  - **Oracle:** `src/renderer/Thread.zig:203, 210, 300-303, 340, 462-463, 480, 501, 522, 573, 599, 656, 684-686, 719, 822` → **Rustty:** `crates/rustty-core/src/renderer_seam.rs:1025-1029, 1034, 1136-1137, 1177-1178, 1255, 1551-1553, 1578, 1588, 1627, 1689, 1723, 1776, 2064`
  - **Audit's suggested fix:** Wire the module's commented log calls to the workspace logging facility once one exists (single sweep).
- [ ] **S115-F4** · CONFIRMED · `LOG` · other
  - **What:** All log.warn/info/debug/err statements in this range are commented out (no logging in rustty-renderer)
  - **Oracle:** `src/renderer/generic.zig:852, 991, 1169, 1177, 1183, 1188, 1190, 1320, 1436, 1472, 1506, 1517, 1533, 1558, 1595` → **Rustty:** `crates/rustty-renderer/src/generic.rs:2698-2721, 4977, 5137, 5186, 5214-5229, 5248, 5273, 5316, 5898`
  - **Audit's suggested fix:** Route these through the workspace logging facade once one is chosen for rustty-renderer.
  - **Disposition (task-14c-1):** WAIVED — 14/15 cited anchors restored as live `log::` sites; `generic.zig:1558` has no reachable `catch` arm (rustty `rebuild_cells` is infallible). See `ADJUDICATION.md#s115-f4`.
- [ ] **S116-F2** · CONFIRMED · `LOG` · other
  - **What:** log.warn/log.debug sites are comments or eprintln (no logging facade); open/read warnings merged
  - **Oracle:** `src/renderer/generic.zig:1960-2007, 2137, 2627, 2777-2781, 2912-2916, 3143-3154, 3181-3185, 3222-3226, 3237-3241, 3461, 3476, 3481, 3514, 3518` → **Rustty:** `crates/rustty-renderer/src/generic.rs:2932, 3126-3131, 3157-3168, 3473, 3648-3650, 4200-4202, 4210-4212, 4294-4296, 4308-4310, 4633, 4659-4665, 4713-4718`
  - **Audit's suggested fix:** When the logging facade lands, wire these sites with the oracle's exact messages (two separate open/read messages) and carry the upload error through instead of collapsing it to MetalFailed.
  - **Disposition (task-14c-1):** WAIVED — 16/18 cited anchors restored as live `log::` sites (including the four `eprintln!` stand-ins and the un-merged open/read messages); `generic.zig:2912`/`:3181` have no reachable `catch` arm (rustty `ShaperCache::put` is infallible). See `ADJUDICATION.md#s116-f2`.
- [ ] **S117-F3** · CONFIRMED · `LOG` · behavioral-difference
  - **What:** All log.warn/log.info diagnostics in image.zig and shadertoy.zig are elided (comment-only), no logging facade
  - **Oracle:** `src/renderer/image.zig:85,122,132,165,365,378-381,396,423,447,486,646-649,660,747` → **Rustty:** `crates/rustty-renderer/src/image/macos_impl.rs:218,257,266,289,696,723,738,773,895,1200,1215,1339`
  - **Audit's suggested fix:** Wire a `log`/`tracing` facade (scoped `renderer_image` / `shadertoy`) and emit the same messages at the same sites; at minimum forward the spirv-cross callback message and the glslang info/debug logs so shader compile errors reach the user.
  - **Disposition (task-14c-2):** WAIVED — 11/13 cited `image.zig` anchors restored as live `log::` sites under `renderer_image`, and all three of `shadertoy.zig`'s live sites (`:71`, `:117`, and the unclaimed spirv-cross callback at `:298`). `image.zig:365` (`Vec::push`) and `:447` (`HashMap::entry`) are infallible in rustty, so neither `log.warn` has a failure arm to live in. See `ADJUDICATION.md#s117-f3`.
- [x] **S118-F4** · CONFIRMED · `LOG` · behavioral-difference
  - **What:** Dropped log sites: Pipeline checkError, Frame present failure, IOSurfaceLayer wrong-size discard
  - **Oracle:** `src/renderer/metal/Pipeline.zig:195-204` → **Rustty:** `crates/rustty-renderer/src/metal/pipeline.rs:182-194`
  - **Audit's suggested fix:** Wire the renderer to the workspace log facade (or `log` crate) and emit the three messages with the same text/level.
  - **Disposition (task-14c-2):** CLOSED — the cited anchor (`Pipeline.zig:195-204`) plus both sites named only in the row's title are live. Pipeline `checkError` → `metal::pipeline::check_error`. `Frame.zig:86`'s present failure is NOT in `metal/frame.rs`: the port inlines `bufferCompleted` into `generic.rs::metal_buffer_completed`, so the emission is there — carrying `crate::metal::frame::LOG_SCOPE` (`"metal"`), because the scope belongs to `Frame.zig`, not to `generic.zig`. `IOSurfaceLayer.zig:115`'s wrong-size discard is live in `metal/iosurface_layer.rs` under the reference's own `IOSurfaceLayer` scope casing.
- [x] **S119-F1** · CONFIRMED · `LOG` · behavioral-difference
  - **What:** Metal shader log sites (checkError log.err, post-shader log.warn, library-load log.debug) are not emitted
  - **Oracle:** `src/renderer/metal/shaders.zig:151, 350, 439-447` → **Rustty:** `crates/rustty-renderer/src/metal/shaders.rs:40-42, 374-382, 419-452, 505-507`
  - **Audit's suggested fix:** Wire the crate's log facade (once available) and emit the three messages, extracting `localizedDescription` from the NSError in the map_err closures.
  - **Disposition (task-14c-2):** CLOSED — all three of `shaders.zig`'s live calls are live in `metal/shaders.rs`. `:350`'s `shader library loaded time={}us` needed its MEASUREMENT restored too (the port had commented out the `std.time.Instant` pair along with the log); it now uses `std::time::Instant`. `:151`'s `err={err:?}` always renders `MetalFailed` because the port collapsed the whole `initPostPipelines` error set to one variant — a pre-existing error-type divergence, noted at the site, not a logging one, and tracked in its own right as `NEW-FINDINGS.md` [[N8]].
- [ ] **S119-F3** · DISPUTED · `LOG` · missing-branch
  - **What:** OpenGL Frame.complete tail skips api.present when the API owns the context (offscreen) and drops the present-failure log.err
  - **Oracle:** `src/renderer/opengl/Frame.zig:62-70` → **Rustty:** `crates/rustty-renderer/src/generic/linux_impl.rs:812-822`
  - **Audit's suggested fix:** Emit the log.err once a facade exists; document/keep the owns_context gate only for the rustty-only offscreen harness.
