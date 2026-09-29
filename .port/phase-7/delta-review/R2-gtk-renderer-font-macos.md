# Phase 7 delta review — R2: GTK apprt, renderer, font, macOS

Reviewer R2. Range `20abdb50a..12752b2ac` (`origin/main` in `../ghostty`, oracle HEAD untouched at
the pin). Read-only review: upstream read with `git diff 20abdb50a origin/main -- <path>` /
`git show <sha>`; nothing built, no Rustty code changed.

**Scope.** `src/apprt/**`, `src/renderer/**`, `pkg/opengl/**`, `vendor/glad/**`, `passthrough.glsl`,
`src/font/**`, `pkg/{harfbuzz,freetype,spirv-cross,glslang,dcimgui,gtk4-layer-shell,macos}`,
`macos/Sources/**`, `src/Surface.zig`, `src/surface_mouse.zig`.

**Size of the slice.** 62 non-merge commits touch these paths (the list is
`git log --no-merges 20abdb50a..origin/main -- <the paths above>`). They change 88 files,
+6,237/−4,436. Of that, `src/font/nerd_font_{attributes,tables}.zig` is about 5.2k lines of
table churn.

**Overlap with other reviewers.** CSI 8 t (`714fe9b90`, `219cca932`) also touches
`src/terminal/stream.zig`, `src/config/Config.zig` and `src/termio/stream_handler.zig`, and the
build commits touch `src/build/SharedDeps.zig`. This record covers only the apprt, core-surface,
renderer and Swift halves. The vt, config and termio halves belong to the reviewer who owns
those paths, and they come first in the ordering (§W-6).

Status words: **absent** (nothing in Rustty), **partial**, **present**, **N/A** (Zig build
plumbing with no Cargo counterpart), **superseded** (a later commit in the range reverts it;
port only the end state).

---

## 1. Summary table

`obs?` means observable to a user: **Y** = yes, **perf** = speed only, **log** = log text only,
**int** = internal.

| # | commit | subject | theme | obs? | Rustty status | size | risk |
|---|---|---|---|---|---|---|---|
| 1 | `2f0b65346` | gtk,opengl: free us from the clutches of GtkGLArea | T1 redesign | Y | absent | **L** | **high**: new GTK widget, renderer threading and EGL ownership; no unit gate |
| 2 | `fa2ec3a6a` | pkg/opengl: add EGL headers and bindings | T1 | int | partial (khronos-egl `dynamic` already loads `libEGL`, but lacks the DMA-BUF export extension fns) | M | medium: extension fns via `get_proc_address` |
| 3 | `a5f604da7` | renderer: LatestFrame no-ops when ExportedFrame is void | T1 | int | absent | S | low |
| 4 | `4c8983586` | gtk: initialize core surface on first resize | T1 | Y | absent (Rustty's lazy init is tied to GLArea) | S | folded into #1 |
| 5 | `b87af8646` | pkg/opengl: replace cImport with translate-c | T9 build | int | N/A | — | none |
| 6 | `adf161473` | pkg/opengl: polish API | T3 GL wrapper | int | absent | M | low: mechanical; fixes half of UD40 |
| 7 | `45aa963d6` | opengl: delay opengl flush | T1 | Y | superseded by #16 | — | — |
| 8 | `ce5d51f46` | gtk/render_surface: report dmabuf build errors | T1 | log | absent | S | folded into #1 |
| 9 | `202fa8e39` | gtk: do not disable OpenGL ES and Vulkan | T5 GTK misc | Y (GDK env) | superseded by #22 | — | — |
| 10 | `d00a49827` | opengl: flip Y axis during framebuffer blit | T1 | Y | superseded by #54 (reverted) | — | — |
| 11 | `72cf50855` | renderer: never stop the display link while holding the draw mutex | T4 renderer misc | **Y (macOS deadlock #14150)** | absent (deadlock ported faithfully) | S–M | **high value**: reachable deadlock; macOS twin |
| 12 | `c0c5473da` | build: refactoring our use of translate-c | T9 | int | N/A | — | none |
| 13 | `2b3b89391` | deps: update translate-c backport | T9 | int | N/A | — | none |
| 14 | `fe9cf6a26` | build: fully transition away from cImport/addTranslateC | T9 | int | N/A (x11/dcimgui comment-only hunks) | S | G3b comment realign only |
| 15 | `ef7edc4a2` | gtk/build: speed up repeated builds (build cache) | T9 | int | N/A in effect; `crates/rustty-apprt/build.rs` ports `blueprint.zig`, whose header changes | S | G3b comment realign |
| 16 | `a925a97e3` | renderer/opengl: gl.finish() between present and sharing the frame | T1 | Y | absent | S | folded into #1 |
| 17 | `ab34b8b13` | opengl: explicitly set EGL_SURFACE_TYPE | T1 | Y (Nvidia) | divergent: Rustty's offscreen context asks for `PBUFFER_BIT` | S | medium: must match upstream's `0` |
| 18 | `c15e2f799` | opengl: EGLattrib arrays need no explicit sentinel | T1 | int | N/A (Rust attribute lists carry `NONE`) | — | none |
| 19 | `cc140d478` | opengl: explicitly initialize surfaceless display | T1 | Y (drivers) | present in shape (`eglGetPlatformDisplay(SURFACELESS_MESA)`) | S | low |
| 20 | `8619becb2` | opengl: validate exported DMA-BUF planes | T1 | Y (fixes a crash) | absent | S | adds a candidate fd leak (§6 Q4) |
| 21 | `590ab157f` | opengl/Sampler: handle enum parameters correctly | T3 | int | partial (Rust already passes typed `GLint`) | S | low |
| 22 | `851cd4dc1` | gtk: disable Vulkan again, remove old version gates | T5 | Y (GDK env) | absent | S | **must ship with #1** (§3.4) |
| 23 | `0fca3d34a` | gtk/imgui_widget: remove glClearColor (+ adds `passthrough.glsl`) | T5 | Y | absent | S | `passthrough.glsl` looks stray (§6 Q8) |
| 24 | `a93a8b03b` | gtk,imgui: allow a GLES context | T5 | Y | absent | S | must ship with #22 |
| 25 | `9fc8d9ebd` | gtk: streamline surface overrides (`Overrides.zig`) | T5 | int | partial (`TabOverrides` exists) | S | G3b: new file's comments |
| 26 | `6ab34cad2` | gtk: remove unused blueprint `using` | T5 | int | absent | S | ships with #1's `surface.blp` rewrite |
| 27 | `45013faf8` | gtk: support fractional surface scaling | T2 fractional scale | Y | absent | M | +7 tests (end state of `scale.zig`) |
| 28 | `1964ed123` | gtk: harden fractional scale notify and DPI rounding | T2 | Y | absent | S | core DPI `@round` also affects macOS |
| 29 | `df2186dce` | gtk: drop 4.12 version checks | T2 | int | absent | S | fine at the v4_14 baseline |
| 30 | `08931f405` | gtk: added logs to debug scaling values | T2 | log | absent | S | — |
| 31 | `9dc7408be` | gtk: snap fractional texture origin to device pixels | T2 | Y | absent | S | needs graphene |
| 32 | `0070f9d0a` | gtk: snap fractional texture origin GdkSurface | T2 | Y | absent | S | — |
| 33 | `70a75ceb5` | gtk: Layout unifies scale, origin, snapped size | T2 | int | absent | S | — |
| 34 | `d1da18350` | gtk: remove widgetDeviceSize, snapshotOrigin wrappers | T2 | int | absent | S | — |
| 35 | `e1916c1ab` | gtk: demote log from info to debug | T2 | log | absent | S | — |
| 36 | `47693cc4b` | gtk,renderer: don't export DMABUFs when the apprt can't accept them | T1 | Y | absent | M | new renderer message; core API |
| 37 | `b9e07f98f` | gtk,opengl: flip CPU rendered textures correctly | T1 | Y | absent | S | candidate y-snap defect (§6 Q3) |
| 38 | `4eb308831` | gtk: don't tell systemd ready before SIGUSR2 is handled | T5 | Y (systemd `notify-reload` units) | absent | S | low |
| 39 | `219cca932` | gtk: support resizing the window with CSI 8 t | T5/T6 | Y | absent | M | needs the vt/config/core CSI 8 t pieces first |
| 40 | `714fe9b90` | terminal,macos: CSI 8 t window resize | T6/T8 | Y | absent | L (whole feature) | ABI: new action tag; reachable UD59-class arithmetic |
| 41 | `a1bf5b5e1` | embedded: route split and close C APIs through performBindingAction | T6 core | minor (log text; binding dispatch) | absent | S–M | realigns UD31's embedded half |
| 42 | `5beb94c16` | renderer: apply font-thicken to IME preedit text | T4 | Y | absent (both twins pass `default()`) | S | twin files |
| 43 | `5dfb67298` | surface: restore mouse_shape when modifier overrides end | T6 | Y | absent | S | — |
| 44 | `6674aa3ba` | surface: update keyToMouseShape tests | T6 | test | absent | S | — |
| 45 | `3a766ccf5` | surface: I-beam while shift is held without mouse tracking | T6 | Y | absent | S | — |
| 46 | `0fb6d2940` | SurfaceMouse: simplify keyToMouseShape | T6 | Y | absent | S | test sub-blocks 13→12 |
| 47 | `b15fe2eea` | font: store CodepointKey as a packed u64 | T7 font | perf | absent | S | no font bench gate |
| 48 | `f3276131f` | font: store glyph cache keys as packed u64 | T7 | perf | partial (same equality, old layout, SipHash) | S | 2 comments must be deleted |
| 49 | `f5c056d30` | font: import Constraint from Glyph.zig in codegen | T7 | int | N/A (generator not ported) | — | — |
| 50 | `a3f3331d4` | font: store nerd-font constraints in a 3-level table | T7 | perf | absent | M | +1 test; new font→unicode edge |
| 51 | `b60eed139` | font: append BMP codepoints as one UTF-16 unit | T7 | perf | absent | S | macOS lane only |
| 52 | `d4a5ff58b` | macOS: fix window cascading | T8 macOS | Y | absent | S | — |
| 53 | `451e224c6` | macOS: clean up for #14106 | T8 | int | absent | S | — |
| 54 | `310797df1` | macOS: fix cascading without affecting other new-window behaviours | T8 | Y | absent | S | — |
| 55 | `a8b0855b6` | macOS: fix cascading for HiddenTitlebarTerminalWindow | T8 | Y | absent | S | — |
| 56 | `aafacb1cb` | macOS: fix flicker on new tab with glass style | T8 | Y | absent | S | — |
| 57 | `e347482fb` | macOS: fix find previous when search is focused | T8 | Y | absent | S | a non-crash bug the port kept (§6 Q6) |
| 58 | `e8936b896` | macOS: follow up cascading fix for #14118 | T8 | Y | absent | S | — |
| 59 | `1a8f331f1` | macOS: implement move_tab_to_new_window | T8 | Y | absent (Swift); header tag present | S | `+list-actions --docs` golden |
| 60 | `74e47a706` | macOS: fix title bar clipping custom font | T8 | Y | absent | M | +102 Swift lines, applies as-is |
| 61 | `e8fe0a5e6` | macOS: fix duplicate new windows from Shortcuts.app | T8 | Y (macOS 27) | absent | S | new Swift file; project membership |
| 62 | `b94b3a056` | macOS: reduce tab bar background opacity on macOS 27 | T8 | Y (macOS 27) | absent | S | compiled out below Swift 6.4 |

Three commits in the range were never in Rustty and are reverted inside the range, so only the
end state matters: `45aa963d6`, `d00a49827` and `202fa8e39`. No commit changes a Swift test file.

---

## 2. Test-count and gate impact

- **New reference tests:** 8.
  - `src/apprt/gtk/scale.zig` adds 7: "deviceSize integer scale", "deviceSize fractional scale",
    "deviceSize rejects non-positive inputs", "snapOffset aligns fractional device origins",
    "snapOffset rejects non-positive scale", "snappedDeviceSize uses origin-aware device spans",
    "snappedDeviceSize integer scale with origin".
  - `src/font/nerd_font_attributes.zig` adds "Constraints lookup table", which walks every
    codepoint up to 0x10FFFF (slow under Miri).
  - `surface_mouse.zig` keeps one `test "keyToMouseShape"`, but its body loses a sub-block (13→12).
    The Rust mirror's PORT NOTE counting 13 sub-blocks must change too
    (`crates/rustty-core/src/surface_mouse.rs:178-420`).
  - The CSI 8 t vt test (`stream.zig`) is outside R2's paths.
- **G1b cannot see them.** `.port/tools/gates/check_test_counts.py` has no GTK row and no nerd-font
  row: the module list stops at `apprt_embedded`, and the font rows are glyf/sfnt/head/metrics/
  atlas/codepoint_map/sprite*. This is the Phase-6 `search/terminal.zig` case again. Either add
  rows (`apprt_gtk_scale`, `font_nerd_font_attributes`) or rely on `audit_test_coverage.py`, the
  live per-test audit, as the meter.
- **G3b must re-green.** New reference comment text lives in `render_surface.zig` (large module
  doc), `scale.zig`, `Overrides.zig`, `Dmabuf.zig`, `pkg/opengl/egl.zig`, `OpenGL.zig`/`Target.zig`
  (both rewritten), `generic.zig` (`LatestFrame`, `drawFrameLocked`, `syncDisplayLink` doc), and
  `nerd_font_tables.zig` (header). `blueprint.zig`/`gresource.zig` comments also change (`ef7edc4a2`).
- **R14.** The new GPU paths need strict-mode pivots and no silent skips:
  - the DMA-BUF export path needs a new resource pivot (e.g. `RUSTTY_REQUIRE_DMABUF`), because a
    host can have GL 4.3 without `EGL_MESA_image_dma_buf_export`;
  - the CPU readback path runs wherever `RUSTTY_REQUIRE_GL` holds.
- **ABI / A5.**
  - `resize_window` is appended to `ghostty_action_tag_e`, so no existing discriminant moves.
  - `crates/rustty-apprt-c/tests/abi_layout.rs:659-676` hard-codes a cardinality of 69 and must
    become 70.
  - The new `ResizeWindow` struct is 8 bytes, so the action union stays 24 bytes
    (`abi_layout.rs:210`).
  - No exported C function is added or removed in R2's slice.
- **Goldens.**
  - `move_tab_to_new_window`'s doc change alters `+list-actions --docs`
    (`.port/tools/windows/golden/list-actions-docs.stdout`).
  - The CSI 8 t config key changes the `show-config` goldens and completions; the macOS agent's
    notes list the files.
  - The OpenGL render-target goldens must be re-checked after the Target rework (§3.2).

---

## 3. T1 — the GtkGLArea → EGL + DMA-BUF redesign

### 3.1 What upstream built (end state)

At the pin, GTK's `GLArea` owned the GL context on the main thread. Upstream now gives rendering
its own context on the render thread and hands finished frames to GTK as textures.

**Renderer (`src/renderer/OpenGL.zig`)**
- `init` (`:49`) loads EGL and creates a **surfaceless** display with
  `eglGetPlatformDisplay(EGL_PLATFORM_SURFACELESS_MESA)` (`:53`). It chooses a config with
  `EGL_SURFACE_TYPE 0` and creates a 4.3 core context, then releases it, all on the main thread.
- The display is **never terminated** (`:113` "Do not destroy the EGL display here as it is shared
  across the entire process").
- `threadEnter` (`:222`) makes the context current on the render thread and loads GL through
  `eglGetProcAddress`. `threadExit` (`:232`) releases it and calls `glad.unload()`, which drops a
  per-thread function table.
- `swap_chain_count` goes from 1 to 3 (`:33`).
- `surfaceInit`, `finalizeSurfaceInit`, `displayRealized`, `presentLastTarget` and
  `gpuResourcesReleased` are gone. `setViewport` is new (`:252`); `setScreenSize` calls it
  (`generic.zig:2222`).

**Present path (`present`, `OpenGL.zig:298`)**
- `present` returns an `ExportedFrame = union{dmabuf: Dmabuf, memory: Memory}`.
- It exports a DMA-BUF only while `presentation_health == .healthy`. Otherwise, or when the
  export fails, it reads the pixels back into memory.

**Render target and export (`opengl/Target.zig`)**
- The target is now texture-backed: an `SRGB8_ALPHA8` render texture and FBO, plus an `RGBA8`
  export texture and FBO. The blit between them is needed because Mesa cannot export sRGB textures.
- `exportDmabuf` (`:148`) blits into the export texture, then calls `eglCreateImage` →
  `eglExportDMABUFImageQueryMESA` → `eglExportDMABUFImageMESA` → `planes.validate()` (`:203`).
- `readPixelsAlloc` (`:224`) is the CPU fallback.
- The render target's format no longer follows `alpha-blending`: it was `.srgba` only when
  `isLinear()`, and is now always sRGB (§6 Q9).

**`src/renderer/Dmabuf.zig` (new, 75 lines)**
- Holds width, height, fourcc, modifier, `premultiplied`, and `Planes`: up to 4 plane fds that the
  struct owns, with `deinit` and `validate`.

**Frame completion (`opengl/Frame.zig` `complete`, `:57`)**
- present, then `gl.finish()` (`:78`), then `pushFrame` (`:82`), then
  `surface_mailbox.push(.redraw)` (`:85`). It never blocks on the apprt.

**Shared renderer (`src/renderer/generic.zig`), which Metal also uses**
- `LatestFrame` (`:992`) is a mutex-guarded single slot where the newest frame wins. It is a no-op
  when `ExportedFrame == void`, which is Metal's case.
- An atomic `presentation_health` with a setter (`:1165`) is fed by a new renderer message
  (`message.zig`, `Thread.zig:402`).
- **Shaders and the swap chain are created lazily on the render thread**:
  `.shaders = .uninit` (`:810`). `Shaders.uninit` is added to both `opengl/shaders.zig` and
  `metal/shaders.zig`.
- **GPU resources are freed only on the render thread**:
  - `threadExit` (`:890`) releases the swap chain, shaders, images and background image under
    `draw_mutex`;
  - `renderCallback` releases them and disarms when `!display_realized` (`Thread.zig:559`);
  - `displayRealized`/`displayUnrealized` (`:950`/`:976`) now only flip flags;
  - `releaseGpuResources` (`:1201`) now expects its caller to hold the lock; `setVisible` takes it.
- `drawFrame` is split into a locked body, `drawFrameLocked` (`:1702`), so `syncDisplayLink` runs
  after the mutex is released (commit #11, §5.1).
- `setScreenSize` now stores the whole `size`, not only the padding.

**Core (`src/Surface.zig`, `src/App.zig`, `src/apprt*.zig`)**
- **Removed:**
  - `Renderer.surfaceInit`, `finalizeSurfaceInit`, and the `threadEnter(...) catch unreachable`
    in `deinit`;
  - `App.Message.redraw_surface`, `redrawSurface` and `hasRtSurface`;
  - `apprt.must_draw_from_app_thread`, with the GTK `App.must_draw_from_app_thread`.
- **Added:**
  - `reportPresentationHealth` (`:927`);
  - a surface message `.redraw` that becomes the `.render` action (`redraw`, `:1745`);
  - `displayRealized`/`displayUnrealized` wrappers (`:2544`/`:2550`); the unrealize one wakes the
    render thread.
- **Changed:**
  - `resize` now also sends `.resize` to the render thread (`:2603`); Termio still forwards its own;
  - `xdpi`/`ydpi` are `@round`ed (`:522`, `:3737`).

**GTK**
- `class/render_surface.zig` (new, 461 lines) defines a `gtk.Widget` subclass,
  `GhosttyRenderSurface`:
  - its `snapshot` (`:149`) takes the latest frame and builds a `GdkDmabufTexture`
    (`rebuildDmabufTexture`, `:316`) or a `GdkMemoryTexture` (`:390`);
  - on `DmabufBuildFailed` it calls `core.reportPresentationHealth(.unhealthy)` (`:168`);
  - it flips vertically for OpenGL (`:193-199`);
  - it draws one texel per device pixel at a snapped origin;
  - it emits a `resize` signal in device pixels and watches `GdkSurface::notify::scale`.
- `class/surface.zig`: `gl_area` becomes `render_surface`, and the six `glarea*` callbacks become
  five `renderSurface*` callbacks (no `render`).
  - `initSurface` runs on the first resize.
  - After it, `setCoreSurface` is called (`:3511`), and if the widget is not yet realized the
    renderer starts unrealized (`surface.displayUnrealized()`, `:3334`).
  - Every `makeCurrent`/`getError` check is gone, and so is the only `setError(true)` call
    (pin `surface.zig:3333`; §6 Q2).
- `ui/1.2/surface.blp`: `GLArea gl_area` becomes `$GhosttyRenderSurface render_surface`.
- `application.zig` `setGtkEnv` (`:3285`): `GDK_DISABLE` is now only `vulkan` (`:3302`). The
  `gl-disable-gles`, `gl-no-fractional`, `vulkan-disable`, `gles-api` and `color-mgmt` flags, and
  their GTK version gates, are removed.
- `imgui_widget.zig` still uses a `GLArea`. It now allows GLES (`allowed-apis: gl | gles`) and
  picks `#version 300 es` when the context is GLES (`:242`).

### 3.2 Where Rustty stands today

Rustty ported the pin's GLArea architecture faithfully and live.

**GTK widget layer**
- `crates/rustty-apprt/src/gtk/class/surface.rs`:
  - `gl_area: TemplateChild<gtk4::GLArea>` at `:386`;
  - `redraw` calls `gl_area.queue_render()` at `:1523`;
  - `get_content_scale` uses the integer `scale_factor()` at `:2010`;
  - `init_surface` calls `make_current`/`error` → `InitError::GlArea` at `:3516-3525`;
  - the `gl_realize`/`gl_unrealize`/`gl_map`/`gl_unmap` handlers are at `:4401-4493`.
  - Separately, `:1963-1964` truncates DPI with `as u16`.
- `crates/rustty-apprt/assets/ui/1.2/surface.blp` still declares the `GLArea`.
- `crates/rustty-apprt/src/gtk/app.rs:485-510` documents the app-thread draw ("GTK's `GLArea`
  cannot draw off the app thread").

**Seams and core**
- `crates/rustty-apprt-api/src/seam.rs:87`: `MUST_DRAW_FROM_APP_THREAD`.
- `crates/rustty-core/src/renderer_seam.rs`:
  - `:1634-1656` is the `must_draw_from_app_thread` branch, which pushes `Message::RedrawSurface`;
  - `:401` builds the renderer with `init_windowed`.
- `crates/rustty-core/src/app.rs:520,551,1053,1069`: `RedrawSurface` and `redrawSurface`.
- `crates/rustty-core/src/surface.rs`:
  - `:3235-3238` is the `threadEnter(...).unwrap_or_else(|_| unreachable!())` in `deinit`;
  - `:3082-3084` is the UD104 comment that relies on OpenGL's `threadEnter` doing nothing;
  - `:2171`/`:8631` truncate DPI.

**Renderer**
- `crates/rustty-renderer/src/opengl/opengl_impl.rs`:
  - `OpenGL::init` (`:247`) is a windowed constructor that owns **no** context, because GDK's is
    borrowed;
  - `present` (`:442`) blits to the default framebuffer;
  - `present_last_target` is at `:512`;
  - `present_to_default` (`:551`) is Rustty-only, used by the live test.
- `crates/rustty-renderer/src/opengl/target.rs` is renderbuffer-backed (`:3`, `:20`).
- `crates/rustty-renderer/src/opengl/frame.rs:147` does `gl::finish()` and then completes.
- `crates/rustty-renderer/src/generic/linux_impl.rs`: `init_windowed` at `:920`, `surface_init` at
  `:1243`, `finalize_surface_init` at `:1293`, `display_realized`/`display_unrealized` at
  `:1920`/`:2010`.
- `crates/rustty-renderer/src/generic.rs` holds the macOS twin: `draw_frame_raw` at `:6320`, with
  the no-redraw path at `:6475-6487`.
- **`generic.zig` is ported twice.** `generic.rs` carries the macOS/Metal instantiation and
  `generic/linux_impl.rs` the OpenGL one (header `linux_impl.rs:1-11`). Every shared change in
  §3.1 must be applied to both files.

**The EGL loader already exists, for tests**
- `crates/rustty-renderer/src/opengl/gl/sys.rs` `OffscreenContext` (`:138-320`) already does
  `get_platform_display(PLATFORM_SURFACELESS_MESA)` (`:56`, `:183`) → bind the GL API → choose a
  config → create a 4.3 core context, through `khronos-egl` with the `dynamic` feature
  (`crates/rustty-renderer/Cargo.toml`, Linux/FreeBSD/Windows deps).
- Two differences from upstream:
  - It chooses with `EGL_SURFACE_TYPE = PBUFFER_BIT` (`:281`); upstream's end state uses `0`
    (`ab34b8b13`, for Nvidia).
  - Its `Drop` calls **`eglTerminate`** (`:317`); upstream deliberately never terminates.

**History that matters here: Rustty once crashed on exactly this pattern.**
`crates/rustty-apprt/tests/gtk_first_light.rs:451-458` records that when "the renderer OWNED a
SEPARATE surfaceless EGL context that shared GDK's EGL display", tearing it down produced "a
DETERMINISTIC `munmap_chunk(): invalid pointer` abort at 'Destroying EGL context'". The
GL-ATTACH fix was to borrow GDK's context. Upstream now goes back to a separate surfaceless
context, and avoids the teardown hazard by **never calling `eglTerminate`**. The port must do
the same. Rustty's offscreen context does terminate, and its own caveat at `sys.rs:309-316`
already warns about sharing.

**The GL loader is process-global in Rustty.** The `gl` crate's `gl::load_with`
(`sys.rs:389`) fills one table for the whole process. Upstream's glad context is thread-local,
and `threadExit` unloads it (`OpenGL.zig:234`). After the redesign, three things would load GL
entry points into Rustty's single table:
- each surface's render thread, from a surfaceless EGL desktop-GL context;
- the GTK imgui widget, from GDK's context, which may now be GLES (`a93a8b03b`);
- any other surface's thread, loading and exiting at arbitrary times.

§6 Q1 asks how to resolve this.

**Rustty-only GTK integration tests built around `GLArea`.** These are not reference tests; the
count below is the number of lines mentioning `GLArea`.

| test file | `GLArea` lines |
|---|---|
| `crates/rustty-apprt/tests/gtk_first_light.rs` | 51 |
| `gtk_window_live.rs` | 15 |
| `gtk_inspector_dump.rs` | 15 |
| `gtk_input_focus.rs` | 10 |
| `gtk_glyph_dump.rs` | 8 |
| `gtk_clipboard.rs` | 1 |
| `gtk_seam.rs` | 1 |

They must move to `RenderSurface`. `gtk_first_light`'s visible-render proof, a `glReadPixels` of
GDK's framebuffer, has to become an inspection of the `GdkTexture` built in `snapshot`.

### 3.3 Mapping the upstream pieces onto Rustty

| upstream piece | Rustty home | nature of the work |
|---|---|---|
| `pkg/opengl/egl.zig` + `vendor/glad/*egl*` | `opengl/gl/sys.rs` over `khronos-egl` | no vendored glad (the R3 precedent is already set). Add `Image` create/destroy (EGL 1.5 `eglCreateImage`, a `khronos-egl` 1.5 feature) and the two `MESA` export fns via `get_proc_address` + transmute. Port the Rust-shaped `Error` mapping |
| `OpenGL.zig` `init`/`deinit`/`threadEnter`/`threadExit`/`setViewport`/`present`/`ExportedFrame` | `opengl/opengl_impl.rs` | merge the offscreen and windowed constructors into one EGL-owning `init`; no `eglTerminate`; delete `present_last_target`/`gpu_resources_released`/`display_realized`; decide what `present_to_default` becomes (Rustty-only) |
| `opengl/Target.zig` | `opengl/target.rs` | full rewrite: renderbuffer → two textures + two FBOs, `export_dmabuf`, `read_pixels_alloc`. UD250's `Framebuffer.bind` quirk now has two new callers (§5.3) |
| `opengl/Frame.zig` | `opengl/frame.rs` + the `linux_impl.rs:826-857` completion | reorder to present, finish, push, `.redraw` |
| `renderer/Dmabuf.zig` | new `crates/rustty-renderer/src/dmabuf.rs` | `OwnedFd`-style ownership; port `validate` faithfully or under R13 (§6 Q4) |
| `generic.zig` (LatestFrame, presentation health, lazy shaders, GPU release on the render thread, `threadExit`, `drawFrameLocked`, `setScreenSize`) | `generic.rs` (macOS) **and** `generic/linux_impl.rs` | twice. The Metal side gets `Shaders::uninit`, loses `presentLastTarget`, and changes lock ownership |
| `renderer/message.zig` `presentation_health` | `crates/rustty-renderer/src/message.rs` | one variant |
| `renderer/Thread.zig` (`app_mailbox` field gone, `must_draw` branch gone, unrealized release in `renderCallback`) | `crates/rustty-core/src/renderer_seam.rs` (the relocated `Thread`, per the stale-deferral memory) | remove the `RedrawSurface` path |
| `Surface.zig` / `App.zig` / `apprt.zig` / `apprt/surface.zig` | `rustty-core` `surface.rs`/`app.rs`, `rustty-apprt-api` `seam.rs`/`surface.rs` | remove the seam items; add `report_presentation_health`, `Message::Redraw`, `display_realized`/`display_unrealized`; fix the UD104 comment at `surface.rs:3082` |
| `class/render_surface.zig` | new `crates/rustty-apprt/src/gtk/class/render_surface.rs` | gtk4-rs `WidgetImpl` subclass: `snapshot`, `size_allocate`, realize/unrealize, a `resize` signal; `gdk4::DmabufTextureBuilder` (available at the `v4_14` baseline, `crates/rustty-apprt/Cargo.toml:277-316`); `gdk4::MemoryTexture` + `glib::Bytes`; register the type before the template is used |
| `scale.zig` | new `crates/rustty-apprt/src/gtk/scale.rs` | pure functions + 7 unit tests, plus the widget helpers (`Surface::scale()` needs 4.12, `Native::surface_transform`, `Widget::compute_point`) |
| `class/surface.zig` + `surface.blp` | `class/surface.rs` + `assets/ui/1.2/surface.blp` | swap the `gl_*` handlers for `render_surface_*`; content scale becomes fractional; `init_surface` loses its GL checks |
| `setGtkEnv`, `imgui_widget.zig`, `imgui-widget.blp` | `class/application.rs:2579-2700`, `class/imgui_widget.rs:477,585-595`, `assets/ui/1.5/imgui-widget.blp:15` | shrink the env to `GDK_DISABLE=vulkan`; GLES `glsl_version` + error path; drop `clear_color` |

**Size of the rework.** It is L, and the largest single item in Phase 7:
- about 1,900 upstream lines change across `OpenGL.zig`, `Target.zig`, `Frame.zig`,
  `generic.zig`, `Thread.zig`, `Surface.zig` and `class/surface.zig`, plus about 800 new lines
  (`render_surface.zig`, `scale.zig`, `Dmabuf.zig`, `egl.zig`);
- the `generic.zig` share must be applied to two Rust files;
- 5 of the Rustty-only GTK live tests need rewriting.

### 3.4 How to stage it

The pieces cannot be merged one at a time without breaking the GTK path for a while. Removing
`must_draw_from_app_thread` and the app-thread `drawFrame` makes the `GLArea` path dead. The
GDK env change (`851cd4dc1` allows GLES) would hand the terminal `GLArea` a GLES context that
Rustty's 4.3-core renderer rejects. The staging below keeps every merge green.

| stage | contents | effect on the tree | verification available |
|---|---|---|---|
| **S0** — decisions | Q1 (GL loader), Q2 (error page), Q4 (DMA-BUF fd leak), Q9 (sRGB target) | none | — |
| **S1** — additive renderer leaves | the `adf161473` GL-wrapper polish (typed enum parameters, error-checked `viewport`, `blit_framebuffer`, `read_pixels`); new `dmabuf.rs`; EGL `Image` + MESA export in `sys.rs`; texture-backed `Target` with `export_dmabuf`/`read_pixels_alloc`; `EGL_SURFACE_TYPE 0`; no `eglTerminate` | the new code is unused by the live path; the offscreen golden moves to the texture target | **headless**: the offscreen OpenGL golden under `RUSTTY_REQUIRE_GL` must stay byte-identical; a `read_pixels_alloc` KAT; an `export_dmabuf` KAT behind a new DMA-BUF pivot (R14) |
| **S2** — renderer lifecycle, both twins, **plus S3 in the same branch** | `ExportedFrame` present, `LatestFrame`, `presentation_health`, lazy shaders, render-thread-only GPU release, `threadExit`, `drawFrameLocked`, `setScreenSize`+`setViewport`, swap chain of 3; core seam removals and additions; `renderer_seam` Thread | the GTK `GLArea` path stops working here, so S2 and S3 must land together | macOS: Metal golden, the display-link test (N291), `xcodebuild` RusttyTests. Linux: renderer unit tests; Miri (`LatestFrame` is a mutex slot) |
| **S3** — GTK `RenderSurface` | `render_surface.rs`, `scale.rs` (+7 tests), `surface.rs`/`surface.blp`, env shrink, imgui GLES, the 5 live tests rewritten | GLArea deleted from the terminal surface | Linux lane (`rvm`) only. **Broadway**: GDK has no DMA-BUF import there, so it exercises the `DmabufBuildFailed` → unhealthy → CPU-readback fallback end to end. **Wayland/X11 on `rvm`**: DMA-BUF export and import. `rvm` exposes a virtio-gpu render node (`/sys/class/drm/renderD128` → `virtio-pci`, `1AF4:1050`), Mesa 26.1.4 and GTK 4.22.4, measured 2026-09-28 over ssh. Whether surfaceless EGL there offers `EGL_MESA_image_dma_buf_export` was **not measured**: `eglinfo` is not installed |
| **S4** — fractional scaling / snapping | already the end state of `render_surface.zig`/`scale.zig`, so it lands with S3; listed separately only for review | — | the 7 `scale` unit tests (pure, all hosts); live check at a fractional scale needs a compositor that can set one (a headless mutter/weston on `rvm`) |

S1 can be merged alone. S2 and S3 are one branch and one merge. Commit #11 (the display-link
deadlock) should land **before** S2 as its own small change, because S2's `drawFrameLocked`
builds on it (§5.1).

---

## 4. T2 — fractional scaling (GTK)

Nine commits: `45013faf8`, `1964ed123`, `df2186dce`, `08931f405`, `9dc7408be`, `0070f9d0a`,
`70a75ceb5`, `d1da18350`, `e1916c1ab`. Port the end state of `scale.zig` and
`render_surface.zig` only.

**What changes for the user**
- Content scale becomes `gdk_surface_get_scale` (fractional) instead of the integer
  `gtk_widget_get_scale_factor` (`surface.zig:1496`, `:1544`).
- Buffers are sized `round(end) - round(start)` in device pixels, and the texture origin snaps to
  a device pixel, so text is no longer blurry at fractional scales.
- `GDK_DEBUG=gl-no-fractional` is dropped (§3.1).
- In core, `xdpi`/`ydpi` are rounded (`Surface.zig:522`, `:3737`). That is a no-op at integer
  scales, including every macOS scale, because 72×k is an integer.

**Rustty:** absent. Integer scale at `class/surface.rs:2010`, `:6028`, `:6059`; DPI truncated at
`class/surface.rs:1963-1964` and `crates/rustty-core/src/surface.rs:2171-2172`, `:8631`.

**Build note:** `9dc7408be` adds `graphene` to the GTK imports (`SharedDeps.zig:722`).
`graphene-rs` is already a transitive gtk4-rs dependency; `widgetSurfaceOrigin` needs
`graphene::Point`.

---

## 5. Other renderer, GL, GTK and core items

### 5.1 `72cf50855` — display link stopped while holding the draw mutex (macOS deadlock)

**Upstream** (`generic.zig:1679-1705`, `:1223` doc): `drawFrame`'s no-redraw path called
`syncDisplayLink` while holding `draw_mutex`. `CVDisplayLinkStop` is a blocking join on
CoreVideo's IO thread, and the CoreAnimation display callback on the main thread takes the same
mutex (`drawFrame(sync)`), so the two could deadlock (#14150).

**Rustty:** it ported the deadlock faithfully. `crates/rustty-renderer/src/generic.rs:6336`
takes `draw_mutex`, and `:6481-6487` calls `present_last_target` then `sync_display_link` inside
it. No UD entry records it.

**Size:** S–M, in `generic.rs` only, since the Linux twin has no display link.

**Verification:** macOS only; the display-link test from N291 is enforced on the macOS leg. A
deterministic reproduction needs a seam, like UD70's `between_display_stores` hook.

**Ordering:** land first; S2 builds on it.

### 5.2 `5beb94c16` — font-thicken for IME preedit

**Upstream:** `generic.zig:3603` now passes `.thicken`/`.thicken_strength` when rendering preedit
glyphs. `:3459` already did so for normal cells.

**Rustty:** absent in both twins. `generic.rs:4891-4893` and `linux_impl.rs:3610-3612` pass
`..RenderOptions::default()`.

**Size:** S. It changes how preedit renders when `font-thicken` is on (macOS and Linux).

### 5.3 GL wrapper (`adf161473`, `590ab157f`) and related UDs

**What changed**
- The texture target and parameter enums are renamed to snake_case (`.@"2d"`, `.rectangle`,
  `.min_filter`, …). This has no Rust impact beyond comments.
- `Texture.Parameter.Type()` gives typed parameters. Rustty's `gl/sampler.rs:100` and
  `gl/texture.rs:458` take `GLint` and are already type-safe. Port the doc/structure change and
  drop the PORT NOTE about the `unreachable` arm.
- New `draw.readPixels` and `draw.blitFramebuffer`, both error-checked, plus a `BlitMask` packed
  struct.
- `viewport` now checks `getError` (Rustty `gl/draw.rs:251`).

**UD realignments**
- **UD32** (unchecked `BlitFramebuffer` in `present`): `present`'s blit is gone upstream, and the
  new `blitFramebuffer` checks its error (`Target.zig:168`). Mark it fixed upstream by the redesign
  once S1/S2 land; the port's faithful `present` at `opengl_impl.rs:476-485` disappears.
- **UD40** (removed `@typeInfo` tags): the `draw.pixelStore` half is fixed upstream
  (`.comptime_int, .int`). The `Buffer.Binding.attribute` half is still there at `origin/main`
  (`pkg/opengl/Buffer.zig:158-159`, `.Array`). Update the UD's upstream status.
- **UD250** (`Framebuffer.bind` saves the draw binding for every target): still present
  (`Framebuffer.zig` has only the translate-c import hunk). `exportDmabuf` now binds `.read` and
  then `.draw` (`Target.zig:162-166`), and `readPixelsAlloc` binds `.read` (`:225`), so the latent quirk has two more callers. It still has no
  visible effect while both bindings start at 0. Keep it faithful, and update the UD's caller list.

### 5.4 `4eb308831` — systemd READY before the SIGUSR2 handler

**Upstream** (`application.zig:201`, `:564`, `:1390`, `:2835`):
- `startupSignals` now runs before `startupStyleManager`;
- a `systemd_ready` flag stops `reloadConfig` sending `RELOADING=1`/`READY=1` before startup is done.

**Rustty:** absent.
- The order is style manager (`application.rs:564`) then signals (`:568`).
- `reload_config` notifies unconditionally (`:5000-5011`).
- READY is sent at `:3644`.

**Size:** S. Observable under a `Type=notify-reload` user unit; Rustty ships
`app-io.airely.rustty.service` (NEW-FINDINGS mentions `systemd-analyze --user verify` passing).

### 5.5 GTK housekeeping (#22, #23, #24, #25, #26)

- **`851cd4dc1` + `202fa8e39`:** `setGtkEnv` shrinks to `GDK_DEBUG=[opengl]` and
  `GDK_DISABLE=vulkan` (§3.1). Rustty's `set_gtk_env` (`application.rs:2579-2700`) still carries
  every pin gate. S, but **ordered with S3**.
- **`a93a8b03b`:** imgui allows GLES. Its init failure now destroys the ImGui context and returns
  (`imgui_widget.zig:245-250`). Rustty's `imgui_widget.rs:477` ignores the `bool`. S.
- **`0fca3d34a`:** drops the imgui `clearColor(0x28,0x2C,0x34)`/`clear` (Rustty
  `imgui_widget.rs:585-595`). It also adds a 4-line `passthrough.glsl` at the repository root that
  nothing references (§6 Q8).
- **`9fc8d9ebd`:** a new `Overrides.zig` type replaces four copies of an anonymous struct. There is
  no behaviour change: the parse body is byte-identical to the pin's `parseOverrides`. Rustty
  already shares `TabOverrides` (`crates/rustty-apprt/src/gtk/class/tab.rs:43`) and
  `parse_overrides` (`application.rs:2132`). S: file placement (a new `class/overrides.rs`) plus
  G3b comments.
- **`6ab34cad2`:** drops `using Gdk 4.0;` from `surface.blp`; folded into S3.

### 5.6 `219cca932` — GTK half of CSI 8 t

**Upstream:**
- `Window.resizeSurface` (`window.zig:968`) refuses a quick terminal, a maximized, fullscreen or
  tiled window (`isTiled`, `:1078`), more than one tab, or a split.
- Otherwise it converts the device-pixel delta using the **integer** `getScaleFactor` and calls
  `setDefaultSize`.
- A `resize_requested` one-shot (`:1261`) keeps `toplevelComputeSize` from overriding it.
- `application.zig:2917` is the action arm.

**Rustty:** absent. Until then the exhaustive action match at `application.rs:4019` stops
compiling when `ResizeWindow` is added (per the macOS/core agent's survey).

**Size:** M. **Dependency:** the vt/config/core/apprt-api CSI 8 t pieces (other reviewer).

### 5.7 Core-surface items from R2's paths (via the macOS/core survey)

**`surface_mouse.zig` series (`5dfb67298`, `6674aa3ba`, `3a766ccf5`, `0fb6d2940`)**
- When a modifier override ends, the shape goes back to the stored `mouse_shape` (an OSC 22
  pointer) instead of a hard-coded `.default`/`.text`.
- Shift without mouse tracking now shows the I-beam.
- The code is simplified to a single `switch`.
- Rustty `crates/rustty-core/src/surface_mouse.rs:57-140` still has the pin table; the call site is
  `surface.rs:7141-7154`. S. Apply as one combined change.

**`a1bf5b5e1`**
- The embedded `request_close`, `split` and `split_focus` now go through `performBindingAction`,
  with `close()` as the fallback.
- Rustty `crates/rustty-apprt-c/src/lib.rs:2212-2279` calls `close()`/`perform_action` directly.
- It fixes UD31's embedded half upstream (the log text becomes "error focusing split"). Replace
  the PORT NOTE at `lib.rs:2253-2254`/`:2277` and mark that half fixed upstream; the GTK half of
  UD31 is still at `origin/main` `application.zig:3026`.
- S–M; rerun the menu-shortcut parity harness.

### 5.8 Build / translate-c (#5, #12, #13, #14, #15)

These are Zig-build-only: `pkg/*/c.zig` `@cImport` → `@import("<x>_c")`, a vendored `pkg/translate-c`,
and memoized gresources.

**Compile flags:**
- HarfBuzz's flags are identical before and after (the font agent compared
  `pkg/harfbuzz/build.zig` pin `:128-170` with `HarfBuzzC.flags()` at `:180-222`); the only drop,
  `-fblocks`, was translate-only. `crates-sys/harfbuzz-sys/build.rs:69-84` already matches.
- dcimgui, glslang, spirv-cross and freetype only reorganize flags into
  `define_flags`/`all_flags` with the same members.
- No vendored library version changes (harfbuzz stays 11.0.0, gtk4-layer-shell 1.1.0).

**Rustty impact:** comment-only.
- `ef7edc4a2` rewrites the header of `src/apprt/gtk/build/blueprint.zig` ("Checks that
  `libadwaita` is at least…", usage `<major> <minor> <stamp>`). `crates/rustty-apprt/build.rs:3-50`
  quotes the pin header and usage. Realign it under R1; no behaviour change, since build.rs already
  runs `blueprint-compiler` directly.
- `pkg/dcimgui/main.zig` rewords a comment.

---

## 6. Open questions for the maintainer

1. **GL function loader after the redesign.**
   - Upstream loads a thread-local glad context per render thread and unloads it in `threadExit`.
   - Rustty's `gl` crate has one process-global table (`sys.rs:389`). That table would be written
     by N render threads (surfaceless EGL, desktop GL 4.3 core) and read by the imgui widget
     (GDK's context, which may be GLES after `a93a8b03b`).
   - Options, with trade-offs:
     - (a) Keep the global table. It works when all contexts share one libglvnd vendor, but it has
       no `unload` analogue, and GLES and desktop GL resolve through the same pointers.
     - (b) Generate a struct-based `gl_generator` binding: one table per render thread, faithful to
       glad's `threadlocal`, but every GL call site changes.
     - (c) Load imgui through epoxy or a separate table, and keep the renderer global.
   - This decides S1's shape. Recommendation after weighing: (b) is the only one that matches the
     reference's model, but it is roughly M extra; (a) is acceptable only with an R1 PORT NOTE and a
     Linux live check with two surfaces plus the inspector open.
2. **The error page becomes unreachable.** Upstream removed the only `setError(true)` call (pin
   `surface.zig:3333`). An EGL or renderer init failure now only logs "surface failed to
   initialize" (`surface.zig:3431`) and leaves a blank surface; `error_page` in `surface.blp` and
   `setError` (`:2305`) survive as dead code. Is this faithful (port the blank surface), or a
   non-crash defect under R13's evident-intent clause (the page exists to show exactly this
   failure)? Not ruled; needs the controller's call.
3. **Candidate defect, not yet measured: the y snap is inverted under the OpenGL flip.** In
   `render_surface.zig:193-203` the flip (`translate(0, css_h)`, `scale(1,-1)`) is applied
   *before* `appendTexture` at the snapped origin. The texture's y range becomes
   `[-oy, css_h - oy]`, so the y snap offset from `scale.zig` `snapOffset` is subtracted, not
   added. Worked through by reading:
   - the device origin D becomes `2D - round(D)` instead of `round(D)`;
   - that is integer only when D already is, or sits at .5;
   - so on OpenGL, the only GTK renderer, the y-axis snapping from `9dc7408be`/`0070f9d0a` is
     ineffective or doubled.

   It needs a measurement on `rvm` (fractional scale plus tab chrome at a fractional y) before
   anyone rules it an R13 fix or a faithful port.
4. **Candidate latent defect: a DMA-BUF fd can leak.** `Dmabuf.Planes.validate`
   (`Dmabuf.zig:62-70`) stops at the first invalid fd and closes only the fds *before* it. Valid
   fds after an invalid one (count > 1, a middle plane failing) leak. Also,
   `rebuildDmabufTexture` leaks all fds if `alloc.create(Planes)` fails (`render_surface.zig:345`,
   before the `errdefer`). Both are driver-dependent or OOM-only. Should they be recorded as UD
   (`kind: leak`, reachability: driver) and fixed in the port under R13, or kept faithful?
5. **Windows OpenGL target.** Rustty compiles the OpenGL renderer for Windows (N342/W-2; the
   `Cargo.toml` comment says the embedded runtime's `surfaceInit` is a no-op there). Upstream's
   `OpenGL.init` now always runs `egl.load()` plus a surfaceless display, so on a host without
   libEGL the renderer, and with it `Surface.init`, fails. Does parity mean Rustty's Windows
   embedded surface init should fail the same way? Whether upstream even links `glad_egl` on
   Windows was not verified.
6. **`e347482fb` (Find Previous ran Find Next).** Rustty kept this non-crash bug faithfully
   (`macos/.../BaseTerminalController.swift:1464-1465` calls `findNext`), although R13's 2026-09-23
   extension should have caught it. The port fixes it anyway. Should it get a retroactive
   `docs/ghostty-bug-fixes.md` note, or none, since upstream now agrees?
7. **CSI 8 t arithmetic.** `Surface.resizeWindow` (`Surface.zig:1923`) repeats UD59's
   `@max(cols,40)*cell.width + padding` → `@intFromFloat(@ceil(w/scale))`, now reachable from
   program output when `vt-window-resize-allowed` is on. Record it as a UD59 extension and port it
   with the checked helpers (`crates/rustty-core/src/surface.rs:331` `u32_from_ceil`)? GTK's
   `resizeDelta` (`window.zig:1005`) does its own float→int as well.
8. **`passthrough.glsl`** at the repository root (added by `0fca3d34a`, 4 lines, referenced by
   nothing) looks like a stray test shader. Mirror it for tree parity, or record it N/A?
9. **The render target is always sRGB.** Upstream `Target.init` always renders into
   `SRGB8_ALPHA8`; the pin used `.rgba` unless `blending.isLinear()`. On Linux the default is
   `linear-corrected` (`Config.zig:408-412`), so defaults are unaffected. With
   `alpha-blending = native` on Linux, the output may change, since the shader output is now
   sRGB-encoded by the framebuffer. Measure with the offscreen golden set to `native` before
   porting, then decide between faithful and R13?
10. **Timing of the macOS deadlock fix (`72cf50855`).** It is a reachable deadlock in shipped
    Rustty (§5.1). Port it ahead of the Phase-7 advance as a standalone fix, or keep it inside the
    resync?

---

## 7. Proposed workstreams, ordering and effort

Effort is in focused worker-days, including gates and negative controls, and excluding waiting
on long CI runs.

| WS | contents | depends on | size | days |
|---|---|---|---|---|
| **R2-A** macOS deadlock | `72cf50855` in `generic.rs`; seam test | — | S–M | 1–2 |
| **R2-B** font | `b15fe2eea`+`f3276131f` (one change, one custom `write_u64` hasher; delete the two stale comments); `b60eed139` (coretext, macOS lane); `a3f3331d4` (transcribe the 3 arrays to `generated/`, add the `rustty-font`→`rustty-unicode` edge, which is acyclic because unicode is a leaf; +1 test; `#[inline]` + const-generic sizes; measure against the `match` since there is no font bench gate) | — | M | 3–4 |
| **R2-C** core small | `surface_mouse` series; `a1bf5b5e1` + UD31 realign; `5beb94c16` (both twins); DPI `@round` | — | S | 2 |
| **R2-D** GL leaves (S1) | `adf161473`/`590ab157f` wrapper; `dmabuf.rs`; EGL image/export; texture `Target`; surface type 0; no terminate; UD32/UD40 status updates | Q1, Q4, Q9 | M–L | 4–5 |
| **R2-E** renderer lifecycle + GTK RenderSurface (S2+S3+S4, one merge) | §3.3 rows for generic (×2), Frame, OpenGL, message, renderer_seam Thread, core Surface/App, apprt-api; `render_surface.rs`, `scale.rs` (+7), `surface.rs`/`surface.blp`, env, imgui GLES, 5 live tests rewritten; commits #1, #3, #4, #7, #8, #9, #10, #16, #20, #22, #23, #24, #26, #27–#37 | R2-A, R2-D, Q2, Q3 | **L** | 12–16 |
| **R2-F** GTK small | `4eb308831` systemd; `9fc8d9ebd` Overrides; `219cca932` CSI 8 t GTK; `ef7edc4a2`/dcimgui comment realign | the other reviewer's CSI 8 t vt/config/core pieces (for 219cca932) | S–M | 2–3 |
| **R2-G** macOS Swift | cascading ×5 (in date order), `e347482fb`, `aafacb1cb`, `74e47a706`, `1a8f331f1` (+`binding.rs:983-984` doc + list-actions golden), `e8fe0a5e6` (new file; check `project.pbxproj` membership), `b94b3a056`, then the CSI 8 t Swift half on top | CSI 8 t core for the last item | M | 3 |
| **R2-H** finale for this slice | G1b rows for `gtk/scale.zig` + nerd-font; G3b re-green; A5/ABI cardinality 70; goldens; **Linux lane early** (the Phase-6 lesson): `cargo test --workspace`, G-GTKFEAT, broadway fallback run, Wayland DMA-BUF run; macOS `xcodebuild` RusttyTests + menu-shortcut parity | all | M | 2–3 |

**Order:**
1. R2-A.
2. In parallel: R2-B, R2-C, R2-G (Swift parts not needing CSI 8 t), R2-D.
3. R2-E.
4. R2-F (after the CSI 8 t core lands).
5. The CSI 8 t Swift half.
6. R2-H.

R2-E is the critical path. The disk budget allows at most 3 building worktrees at once
(memory `parallel-worktree-disk-budget`).

**Total:** about **29–38 worker-days**. R2-E alone is 12–16, and it is the only part the unit
gate cannot cover. By the heavy-tier doctrine it lands on compile plus the headless golden plus
live-session checks on `rvm`.

**New sys crates:** none. `khronos-egl` (`dynamic`) replaces upstream's vendored
`glad_egl.{c,h}` + `eglplatform.h` (the same R3 choice as today; the `gl` crate already stands in
for glad). The EGL MESA export functions are obtained with `eglGetProcAddress`. No libglvnd or
libgbm link edge: upstream's comment says GBM, but the actual export goes through
`EGL_MESA_image_dma_buf_export`, not libgbm (`Target.zig:181-203`).

**Header/ABI:** only CSI 8 t's `RESIZE_WINDOW` tag and struct (appended). The redesign itself
adds no C API: `reportPresentationHealth` and `takeFrame` are Zig-internal between core and GTK.
