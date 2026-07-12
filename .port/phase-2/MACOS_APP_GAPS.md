# macOS App Gap Audit — Scope + Fix Plan

Synthesis of 6 auditor gap lists for the live windowed macOS app (window opens, shell spawns, keystrokes reach the shell, **but the window is blank**). Read-only audit; nothing was built/run.

---

## 1. Verdict

The app is **one wiring fix away from visible pixels**, and a **small handful of graceful-degrade guards away from crash-free normal use**. The entire render pipeline is ported and LIVE: font discovery, CoreText rasterization, atlas upload to a real `MTLTexture`, `draw_frame_raw` encode, the render thread, the io thread, the xev kqueue loop, termio PTY read, `queueRender`→`renderer_wakeup.notify`, `update_frame`→`drawFrame`, and the CVDisplayLink vsync path — **all six auditors independently traced their chain to the same single break**. The break is not an unported subsystem: it is that `Renderer::init` builds Metal via `Metal::init_offscreen` (`layer=None`), so every encoded frame is presented into an offscreen `IOSurface` and discarded by the `None => Ok(())` no-op arm of `present()`. The windowed constructor `Metal::init(view, scale, blending)` — which sends `setLayer:`/`setWantsLayer:`/`setContentsScale:` and builds `layer: Some(IOSurfaceLayer)` — **is fully implemented but has no caller on the surface-creation path**. This is W-ATTACH (DEFERRALS.md:3677 / plan Task 23).

**Critical path, in order:**
1. **W-ATTACH** — route the NSView into the renderer so `layer=Some`. This alone makes pixels appear.
2. **present() / display-callback / surface_size** — all become live *automatically* once `layer=Some` (they are correct on the `Some(layer)` arm; the no-op arms are the offscreen fallbacks). Requires the attach to run **before** the render thread spawns (`surface.rs:2304`) so `loop_enter` observes `layer=Some`.
3. **On-feature crash guards** — the ALWAYS-reachable normal path (launch/type/resize/close/scroll/select) contains **zero** live `unimplemented!()`. But **quicklook_font/_word** (Look-Up / force-click, a common trackpad gesture) and the **inspector** abort under `panic=abort`. These are not blank-screen blockers but are near-path crashers worth de-risking to a null/false return.

Nothing in xev, termio, core orchestration, font, or input needs to change for pixels. The render loop is not unported — it is unattached.

---

## 2. Blocker chain (must-fix for pixels + no-crash normal use)

Ordered by dependency. Only **B1** is a true blocker for pixels; **B2–B4** are consequences that resolve with B1 but must be verified; **B5** is the only additional item for crash-free *normal* use (and it is on-feature, not strictly normal-path).

### B1 — W-ATTACH: route NSView into the renderer (THE root cause)
- **File:** `crates/rustty-core/src/renderer_seam.rs:240` (drops the host surface) → `crates/rustty-renderer/src/generic.rs:1840` (`Metal::init_offscreen`, `layer=None`).
- **What to wire:** Add a windowed-attach method on `rustty_renderer::Renderer`, e.g. `attach_host_view(&mut self, view: &objc2::runtime::AnyObject, scale: f64)`, that builds the `IOSurfaceLayer` and performs the `Metal.zig:110-126` layer-hosting attach (reuse `Metal::init`'s body: `setLayer:`/`setWantsLayer:`/`setClipsToBounds:`/`contentsScale`/`needsDisplayOnBoundsChange`), then sets `self.api.layer = Some(layer)` — keeping the already-built offscreen device/queue/shaders. Only the renderer crate may do this objc2 work (R4 acyclicity).
- **Call site:** In `renderer_seam::Renderer::init` (or `surface.rs` `Surface::init` right after `Renderer::init`), read `rt_surface.platform.macos.nsview` (`embedded.rs:708`) + `content_scale` (`embedded.rs:902`) and invoke the attach — thread them through a renderer_seam hook mirroring the `RendererConfig` accessor-trait pattern so rustty-renderer never names an apprt type.
- **Ordering constraint:** MUST run **before** the render thread spawn (`surface.rs:2304`) so `loop_enter` sees `layer=Some`. Otherwise expose a re-arm path that re-runs `Metal::loop_enter` after attach.
- **Depends on:** nothing. Head of the chain.
- **Verify:** un-ignore `crates/rustty-apprt-c/tests/macos_window_host.rs::hosts_view_layer_and_presents_live_frame`.

### B2 — present() no-op arm (auto-resolves with B1)
- **File:** `crates/rustty-renderer/src/metal/metal_impl.rs:800` (`None => Ok(())`).
- **What to wire:** Nothing. The `Some(layer)` arm (`:786`) is correct and goes live the instant `layer=Some`. **Verify** `present` errors (`ObjCFailed`) propagate once live.
- **Depends on:** B1.

### B3 — display callback install (auto-resolves with B1, ordering-sensitive)
- **File:** `crates/rustty-renderer/src/metal/metal_impl.rs:505` (`if let Some(layer) … set_display_callback`).
- **What to wire:** Nothing new — but the CoreAnimation-commit redraw (resize / `needsDisplayOnBoundsChange`) only installs when `layer=Some` at `loop_enter` time. Guaranteed by B1's ordering constraint. If attach is wired *after* the thread ran `loop_enter`, add a re-arm.
- **Depends on:** B1 (+ ordering).

### B4 — surface_size frozen on the None arm (auto-resolves with B1; post-attach verify)
- **File:** `crates/rustty-renderer/src/metal/metal_impl.rs:732` (None arm returns fixed `offscreen_size`).
- **What to wire:** Nothing — the `Some(layer)` arm (`:720`) reads `layer.bounds * contentsScale`. **Verify** on live resize that the hosted layer frame auto-tracks the NSView bounds (AppKit manages a layer-hosting view's frame; the golden test sets it manually at `generic.rs:6586`). If it does not auto-track, wire the view resize to update the layer frame.
- **Depends on:** B1.

### B5 — ALWAYS-reached live `unimplemented!()`: **NONE on the normal path**
- Confirmed by all auditors: `performBindingAction` (~90 arms), all key/mouse/scroll/text/preedit/IME callbacks, `cval()` (~65 action variants), clipboard, resize, config, close — **zero** `unimplemented!()`/`todo!()` on launch/render/type/resize/close/scroll/select. The only renderer `unimplemented!()` (`generic.rs:4566`, inspector `overlayFeatures`) is statically dead (uninhabited enum). **No crash-guard work is required for pixels + normal use.** The crashers below (§3) are all on-feature.

**Net:** the blocker chain for pixels is effectively **B1 alone**; B2–B4 are verifications that fall out of it.

---

## 3. High / medium / low ledger (on-feature crashes + polish)

### HIGH — on-feature crashes (abort under `panic=abort`)
| Item | File | Trigger | Note |
|---|---|---|---|
| **quicklook_font** | `rustty-apprt-c/src/lib.rs:2628` | Force-click "Look Up" **and** any VoiceOver / accessibility query (`SurfaceView_AppKit.swift:1521,1949,2361`) | Borderline blocker: accessibility path fires without explicit user intent. Interim: return `null` (Swift already handles `fontRaw==nil`). |
| **quicklook_word** | `rustty-apprt-c/src/lib.rs:2688` | QuickLook/Look-Up word gesture (`SurfaceView_AppKit.swift:1512`) | Interim: return `false` so Swift falls back to `super.quickLook`. Body is objc-free (renderer_state.mutex + selectWord + readTextLocked). |
| **Inspector subsystem** | `rustty-apprt/embedded.rs:1833+`; `rustty-apprt-c/lib.rs` `inspector_metal_*`; `rustty-core/surface.rs:2842/2877` (`activate/deactivateInspector`) | User opens the terminal inspector (cmd-opt-i) | MILESTONE.md lists live inspector as DEFERRED; Swift likely leaves the keybind unbound this milestone. Interim: no-op/return-null leaves. Full fix = dcimgui-sys + ImGui_ImplMetal (Phase-3). |

### MEDIUM — degraded, post-attach
| Item | File | Note |
|---|---|---|
| **contentsScale not re-stamped** on cross-DPI monitor move | `rustty-core/surface.rs:7721` (`contentScaleCallback`) | Only after B1; blurry render on Retina↔non-Retina drag. Mirrors Ghostty structure — may be host-owned (Swift view sets layer scale). **Verify against oracle**; if rustty owns it, add a Metal `set_content_scale(scale)` mailbox message. |

### LOW — polish / correctly-dead / off-platform
- **drainMailbox autoreleasepool** ported as no-op scope (`renderer_seam.rs:949`) — slow ObjC leak under sustained mailbox traffic, no correctness/blank impact. Fix via a `rustty_os` autoreleasepool shim.
- **renderer inspector `overlayFeatures`** (`generic.rs:4566`) — statically unreachable (uninhabited enum); fills with Phase-3 inspector.
- **Backend::childExitedAbnormally** (`termio/backend.rs:180`) — no callers; real path is `Surface::childExitedAbnormally` (`surface.rs:3601`, implemented).
- **xev ThreadPool** (`thread_pool.rs:77`) — type-surface only; PTY uses `EVFILT_READ`, never the pool.
- **xev non_macos_seam** (`lib.rs:344`, ~15 sites) — `#[cfg(not(macos))]`, compiled out.
- **termio Windows/flatpak spawn** (`exec.rs:2437/3426/3022`) — `#[cfg(windows)]` / const-false.
- **font-shaper Fontconfig DeferredFace** (`deferred_face.rs:479`) — `#[cfg(not(macos))]`; macOS uses CoreText.
- **rustty-vaxis** `unimplemented!()` sites — CLI-TUI path, **not linked** into the macOS GUI app.
- **core dummy-backend / app test-runtime** (`lib.rs:137`, `app.rs:1120`) — `#[cfg(test)]` only.
- **Stale doc block** (`generic.rs:11-38`) — claims `DerivedConfig::init`/`setScreenSize`/`changeConfig`/`setFontGrid` are unimplemented seams; all four are landed & real. Update to narrow DEFERRED to W-ATTACH only.
- **Deferred static bold/italic embedded faces**, **FreeType stbir resize**, **Atlas Wasm C-ABI** — off-path on macOS (variable font covers all styles; CoreText resizes via `scale_ctm`).

---

## 4. Fix plan (phased)

### P1 — Pixels on screen + no crash on launch/type/resize/close *(the milestone)*
1. **B1 — W-ATTACH** (`renderer_seam.rs:240` + new `Renderer::attach_host_view` in `generic.rs`/`metal_impl.rs`). Single-threaded critical fix. Ordering: attach **before** thread spawn (`surface.rs:2304`).
2. **B2/B3/B4 verification** (present propagation, display-callback install, live surface_size + resize reflow). Falls out of B1; needs a live resize test.
3. Un-ignore the two windowed-present KATs (`macos_window_host.rs`).
- **P1 is essentially one code change plus verification.** Normal-path requires **no** crash guards (§B5).

### P2 — Common on-feature features (crash-guard first, then implement)
Parallelizable — three independent leaves:
- **2a. Crash-guard quicklook** — swap `unimplemented!()` for `null`/`false` returns at `lib.rs:2628/2688`. Trivial, high value (Look-Up is a common gesture + accessibility). Then port the bodies (`quicklook_word` is objc-free; `quicklook_font` needs CoreText).
- **2b. Crash-guard inspector** — make the E4/embedded + core `activate/deactivateInspector` leaves no-op/null so an accidental keybind degrades gracefully. Full dcimgui-sys inspector is a large Phase-3 item.
- **2c. contentsScale restamp** (medium) — resolve oracle ownership; add Metal `set_content_scale` mailbox message if rustty owns it.

### P3 — Polish
- drainMailbox autoreleasepool shim; stale `generic.rs` doc block; then the correctly-dead/off-platform leaves (Fontconfig, FreeType stbir, Wasm atlas, static faces, xev thread pool, Windows/flatpak) as their respective milestones land.

**Parallelization:** P1 is serial and gates everything visible. Within P2, 2a/2b/2c are independent. P3 items are all independent.

---

## 5. Honest scope estimate

**Fully-functional windowed terminal is a SMALL wiring fix, not a render-loop port.** The render loop, thread orchestration, event loop, termio, font/glyph/atlas pipeline, and the entire input + action surface are **already ported and LIVE** — six independent auditors traced their chains to the *same single unattached seam*. The windowed `Metal::init(view)` body already exists (verbatim from `Metal.zig:64-156`); it simply has no caller. **W-ATTACH is one method + one call site** (plus threading nsview/scale through a renderer_seam accessor hook, ~a day of focused work including the acyclicity plumbing and resize verification).

**Does W-ATTACH alone get visible pixels? YES.** Attaching the layer flips `present()`, the display callback, and `surface_size` onto their already-correct `Some(layer)` arms — the working atlas/glyph pipeline paints immediately. No other change is needed for launch/type/resize/close, because the normal path has **zero** live `unimplemented!()`. The remaining `unimplemented!()` crashers (quicklook, inspector) are **on-feature only** and are P2 graceful-degrade guards, not blockers for a usable terminal.
