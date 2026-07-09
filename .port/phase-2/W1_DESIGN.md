# W1 Design — macOS On-Screen Windowing Sub-project #1, Chunk W1

**Faithful-port design for W1-1..W1-7** (all in `rustty-renderer`, apprt-free). Governing plan:
`docs/superpowers/plans/2026-07-08-macos-windowing-sub1.md` (Chunk W1 + the **Canonical Interfaces** and **Global Constraints** — AUTHORITATIVE). This doc resolves the method-level signatures against the reference and pins the exact edits, objc2 mappings, and runnable KATs.

Reference oracle (read-only): `/Users/gael/projects/ghostty`
- `src/renderer/Metal.zig` — `init` (64-156), `deinit` (158-162), `loopEnter`/`displayCallback` (164-176), `surfaceSize` (216-233), `present` (253-259), `presentLastTarget` (261-264).
- `src/renderer/generic.zig` — `init` (658-798), `deinit` (800-830), `surfaceInit` (864-869), `finalizeSurfaceInit` (873-878).
- `src/renderer/metal/Frame.zig` — `complete` (~104-133), `bufferCompleted` (58-90).

## Global invariants (apply to every task)

- **R1** — every reference comment/behavior ported; the verbatim Zig body of any relocated seam stays as a doc-comment; Zig byte-slices → `&[u8]`/`Vec<u8>`/`CString`, never lossy `String` (no byte-slices arise in W1).
- **R4** — `rustty-renderer` MUST NOT name any apprt type. The windowed host view is passed **type-erased** (`&objc2::runtime::AnyObject`, exactly ghostty's `info.view: objc.Object`) or **generic-over-`S`** — no `objc2-app-kit` dep is added, no apprt edge is introduced. (`cargo tree`/`check_acyclic.sh` gate.)
- **R6** — every W1 test is FFI (objc2/Metal) ⇒ **Miri-excluded, ASan-eligible**; add the new test names to `.port/phase-2/MIRI_EXCEPTIONS.md`. The raw-`*mut` present/completion aliasing is already proven clean under SB+TB on `.port/phase-2/mirihar-chunk3b` (`drawframe_shipped`); W1 adds no new raw-alias shape (the async block in W1-4 uses the same by-value-`Copy` `CompletionContext` the sync path uses).
- **Headless-friendly** — every KAT skips cleanly (`return`) when `Metal::init_offscreen(..).is_err()` (== `MetalError::NoMetalDevice`), matching the W0 KAT. No KAT requires a WindowServer/NSApplication run loop.

## objc2 surface — VERIFIED (removes the plan's one High open-risk)

Confirmed present in the resolved `objc2-quartz-core-0.3.2` `src/generated/CALayer.rs` (build cannot regress — W0's `iosurface_layer.rs` already links `bounds()`/`contentsScale()`/`setContents()`/`setContentsGravity()`):
- `bounds(&self) -> CGRect` (226), `setBounds(&self, CGRect)` (232)
- `contents(&self) -> Option<Retained<AnyObject>>` **unsafe** (459), `setContents(&self, Option<&AnyObject>)` **unsafe** (468)
- `contentsScale(&self) -> CGFloat` (495), `setContentsScale(&self, CGFloat)` (501)
- `setNeedsDisplayOnBoundsChange(&self, bool)` (649)

`CAMetalLayer.rs` exists; the `Cargo.toml` `objc2-quartz-core` features `["CALayer","CAMetalLayer","objc2-core-foundation"]` are already in-lock (W0). **No Cargo.toml change is needed for W1** (`CAMetalLayer` was pre-enabled in W0 "harmless for W1"). NSView selectors (`setLayer:`/`setWantsLayer:`/`setClipsToBounds:`/`layer`/`wantsLayer`) are reached by `objc2::msg_send!` over `&AnyObject` — **no `objc2-app-kit` dep**.

### KAT compile-invariants (apply to EVERY W1-2/W1-3/W1-6 KAT that touches a layer/view)

These are non-obvious `objc2`/borrow requirements every KAT below must satisfy verbatim, or `cargo test` will not compile:
1. **`CALayer::contents()` is an `unsafe fn`** (verified `objc2-quartz-core` `CALayer.rs:459`). EVERY `…as_ca_layer().contents()` read — in the present/handshake asserts — MUST be wrapped: `let c = unsafe { …as_ca_layer().contents() };`. (`setContents`/459-468 is likewise unsafe, but W1 sets contents only via `IOSurfaceLayer::set_surface[_sync]`, which already wraps it.)
2. **`present`/`present_last_target`/`draw_frame` take `&mut self`** ⇒ the on-screen instance must be a `let mut`: `let mut onscreen = Metal::init(&view, …)?;` before `onscreen.present(…)`. A non-`mut` binding fails to compile.
3. **`msg_send![class!(NSView), new]` yields `Retained<AnyObject>`** (not `&AnyObject`). `Metal::init(view: &AnyObject, …)` and `msg_send![&*view, …]` both need the deref-coercion: pass **`&*view`** (`&Retained<AnyObject>` → `&AnyObject`). Binding is `let view: Retained<AnyObject> = unsafe { msg_send![class!(NSView), new] };`.
4. **`msg_send!` calls are `unsafe`** — every `msg_send![view, setLayer: …]`/`[&*view, layer]`/`[&*view, wantsLayer]` sits inside an `unsafe { }` block.

---

## W1-1 — `Metal.layer: Option<IOSurfaceLayer>` field + `None` in `init_offscreen`

**File:** `crates/rustty-renderer/src/metal/metal_impl.rs`.

**Edits:**
1. **Imports (after line 65)** add `use super::iosurface_layer::{self, IOSurfaceLayer};` (the `self` alias is used for `iosurface_layer::Error` in W1-3's present mapping).
2. **Struct field (lines 98-104)** — replace the M-1 `DEFERRED` comment block with the real field, keeping the verbatim Zig decl as its doc-comment (R1):
   ```rust
   // Metal.zig:41 — `layer: IOSurfaceLayer,` (non-optional in ghostty, which always
   // has a host view). SANCTIONED DIVERGENCE (R1-documented): rustty added the
   // offscreen golden path (`init_offscreen`, no view) BEFORE the windowing tier, so
   // the minimal faithful encoding is `Option`: None == offscreen (present no-op,
   // surface_size == offscreen_size), Some == on-screen (layer attached to an NSView).
   pub layer: Option<IOSurfaceLayer>,
   ```
   Place it as the first field (matching Metal.zig's field order: `layer` before `device`).
3. **`init_offscreen` returned literal (241-249)** — add `layer: None,` (first field). Update the seam-M-2 comment at 235-239 to note the layer is now an explicit `None` (offscreen keeps the sanctioned no-layer divergence).
4. **`Drop` (660-664)** — update the M-1 note: `layer` (the `Option<IOSurfaceLayer>`) releases via `IOSurfaceLayer`'s `Retained<CALayer>` Drop; ARC order immaterial. `self.layer.release()` (Metal.zig:161) ⇒ that Drop.

**KAT** (`metal_impl::tests`, macOS): `offscreen_metal_has_no_layer` — `init_offscreen(Native,64,32)` ⇒ `m.layer.is_none()` and `surface_size() == (64,32)` (the `None` branch still returns `offscreen_size`). Skip on `Err`.

**Risk:** Low (pure struct shape). The `Option` divergence must be documented or the completeness critic flags a non-verbatim field.

---

## W1-2 — `Metal::init` on-screen constructor + factor the shared device prologue

**File:** `crates/rustty-renderer/src/metal/metal_impl.rs` (replace the `unimplemented!` stub at 333-337; keep the verbatim body comment 259-332).

### Factor the shared prologue (the "reuse init_offscreen's shared body" directive)

`Metal::init` and `Metal::init_offscreen` share the offscreen-portable half of Metal.zig:64-89 (`chooseDevice` + `newCommandQueue` + `default_storage_mode` + `max_texture_size`). Extract a private helper so neither duplicates it:

```rust
/// The offscreen-portable half of Metal.zig `init` (lines 72-88): chooseDevice +
/// newCommandQueue + device metadata. Shared by `init_offscreen` (layer None) and
/// `init` (windowed, adds the view attach). Returns the four device-derived fields.
fn init_device() -> Result<(Retained<ProtocolObject<dyn MTLDevice>>,
                            Retained<ProtocolObject<dyn MTLCommandQueue>>,
                            api::StorageMode, u32), Error> {
    let device = choose_device()?;                 // errdefer device.release() -> ARC
    let queue = device.newCommandQueue().ok_or(Error::NoMetalDevice)?;  // errdefer -> ARC
    let default_storage_mode = if device.hasUnifiedMemory() {
        api::StorageMode::Shared
    } else {
        api::StorageMode::Managed
    };
    let max_texture_size = query_max_texture_size(&device);
    // log.debug("device properties ...")  // no facade
    Ok((device, queue, default_storage_mode, max_texture_size))
}
```

`init_offscreen` (206-250) is rewritten to `let (device, queue, default_storage_mode, max_texture_size) = init_device()?;` then the same struct literal with `layer: None, offscreen_size: {width,height}`. Behavior identical — the existing `metal_offscreen_construct_and_begin_frame` KAT re-proves it.

### The windowed constructor

Signature (Canonical/plan W1-2 "Produces"): the view + scale are the two values ghostty extracts from `opts.rt_surface` (embedded apprt) — passed here as **concrete already-extracted params**, exactly as `init_offscreen` took `blending`/`width`/`height` directly rather than `Options`. `view` is **type-erased** (`&AnyObject`) — byte-faithful to `info.view: objc.Object`, keeps R4 (no app-kit dep).

```rust
/// Metal.zig `init(alloc, opts)` — the on-screen constructor. `view` is the host
/// NSView/UIView (ghostty `info.view`), `scale_factor` is `opts.rt_surface.
/// content_scale.x` (ghostty `info.scaleFactor`). See the verbatim Zig body above.
pub fn init(
    view: &objc2::runtime::AnyObject,
    scale_factor: f64,
    blending: AlphaBlending,
) -> Result<Metal, Error> {
    let (device, queue, default_storage_mode, max_texture_size) = init_device()?;

    // Create an IOSurfaceLayer ... "layer-hosting view" so we manually control contents.
    // `IOSurfaceLayer::init` returns `iosurface_layer::Error::ObjCFailed` ⇒ fold to the
    // dedicated `Error::ObjCFailed` (NOT NoMetalDevice; W1-3 present error set note).
    let layer = IOSurfaceLayer::init()
        .map_err(|iosurface_layer::Error::ObjCFailed| Error::ObjCFailed)?; // errdefer -> ARC

    // Add our layer to the view. On macOS: assign to `layer` BEFORE `wantsLayer=true`
    // (the ORDER is load-bearing — the Zig comment mandates it). The `.ios` addSublayer
    // arm (Metal.zig:128-131) is a dead comptime arm preserved as a comment (R1).
    let l: *const objc2_quartz_core::CALayer = layer.as_ca_layer();
    unsafe {
        let _: () = objc2::msg_send![view, setLayer: l];
        let _: () = objc2::msg_send![view, setWantsLayer: true];
    }
    // Ensure an oversized layer does not overflow the view bounds.
    unsafe { let _: () = objc2::msg_send![view, setClipsToBounds: true]; }
    // Content scale == window scale factor (avoids blurry magnification).
    layer.as_ca_layer().setContentsScale(scale_factor);
    // So our display callback is actually called.
    layer.as_ca_layer().setNeedsDisplayOnBoundsChange(true);

    Ok(Metal {
        layer: Some(layer),
        device, queue, blending, default_storage_mode, max_texture_size,
        autorelease_pool: None,
        // OFFSCREEN seam M-3 no longer drives surface_size on-screen (layer branch,
        // W1-3); keep a zeroed offscreen_size — it is never read while layer.is_some().
        offscreen_size: SurfaceSize { width: 0, height: 0 },
    })
}
```

Notes: use `layer.as_ca_layer()` (the W0 Canonical upcast) rather than reaching `layer.layer` — the plan mandates consumers take `&CALayer` via `as_ca_layer()`. `errdefer {device,queue,layer}.release()` all map to `Retained` Drop on the `?` early returns. `blending` maps ghostty's `opts.config.blending`.

**KAT** (`init_binds_layer_to_nsview`, macOS): probe-skip; `let view: Retained<AnyObject> = unsafe { msg_send![class!(NSView), new] }` (a bare NSView needs no NSApplication to accept a layer-host handshake); `Metal::init(&*view, 2.0, Native)` (**`&*view`** deref-coerces `&Retained<AnyObject>` → `&AnyObject`); assert `m.layer` is `Some`, `unsafe { msg_send![&*view, layer] }` pointer-equals `layer.as_ca_layer()`, `unsafe { msg_send![&*view, wantsLayer] } == true`, and `layer.as_ca_layer().contentsScale() == 2.0`.

**Risk:** Med. Bare-NSView creation on headless CI *may* need `+[NSApplication sharedApplication]` first; mitigation — if the attach throws, prepend a `let _ = msg_send![class!(NSApplication), sharedApplication];` in a `test_support` helper (see W1-7). Selector spellings `setLayer:`/`setWantsLayer:`/`setClipsToBounds:` must match AppKit (they do).

---

## W1-3 — on-screen `surface_size`, `present`/`present_last_target`, `loop_enter`

**File:** `crates/rustty-renderer/src/metal/metal_impl.rs`.

### `surface_size` (598-603; comment 581-597)
Add the `Some(layer)` branch (Metal.zig:216-232) alongside the existing `None` offscreen branch. Both apply the same `.min(max_texture_size)` clamp:
```rust
pub fn surface_size(&self) -> Result<SurfaceSize, Error> {
    match &self.layer {
        Some(layer) => {
            // bounds = layer.bounds; scale = layer.contentsScale (Metal.zig:217-218).
            let bounds = layer.as_ca_layer().bounds();     // CGRect
            let scale = layer.as_ca_layer().contentsScale(); // CGFloat(f64)
            // @intFromFloat -> `as u32` (trunc-toward-zero, byte-faithful); clamp each axis.
            Ok(SurfaceSize {
                width:  ((bounds.size.width  * scale) as u32).min(self.max_texture_size),
                height: ((bounds.size.height * scale) as u32).min(self.max_texture_size),
            })
        }
        None => Ok(SurfaceSize {
            width:  self.offscreen_size.width.min(self.max_texture_size),
            height: self.offscreen_size.height.min(self.max_texture_size),
        }),
    }
}
```
Update the seam-M-3 comment (581-597): the layer-bounds path is now LIVE (no longer `pkg/macos.graphics` deferred) — `objc2_core_foundation::CGRect`/`CGFloat` replace `graphics.Rect`/`f64`.

### New `Error::ObjCFailed` variant (faithful present error set)

Ghostty's `Metal.present` is `try self.layer.setSurface(...)`, and `IOSurfaceLayer.setSurface` returns `error{ObjCFailed}` (`IOSurfaceLayer.zig:138-145`); so ghostty's present error set is `error{ObjCFailed}`, NOT `NoMetalDevice`. Folding `iosurface_layer::Error::ObjCFailed` into `Error::NoMetalDevice` would MISLABEL a main-thread-dispatch failure as "no Metal device" — a fidelity loss against ghostty's distinct error set. **Add a dedicated variant** to `Metal`'s `Error` (`metal_impl.rs:90`), preserving the existing doc's "the resource constructors return their own leaf `Error`s" note:
```rust
pub enum Error {
    /// `Metal.zig` `error{NoMetalDevice}` — `chooseDevice` found no usable MTLDevice.
    NoMetalDevice,
    /// Ghostty `error{ObjCFailed}` — propagated from `IOSurfaceLayer.setSurface` /
    /// `IOSurfaceLayer.init` (main-thread objc dispatch or subclass alloc failure,
    /// `IOSurfaceLayer.zig:138-145`). Distinct from `NoMetalDevice` (present error set).
    ObjCFailed,
}
```
`iosurface_layer::Error` is already `enum Error { ObjCFailed }` (verified `iosurface_layer.rs:33-34`) ⇒ the fold is an identity map, maximally faithful.

### `present` (648-650; comment 635-647)
```rust
pub fn present(&mut self, target: &Target, sync: bool) -> Result<(), Error> {
    match &self.layer {
        // Metal.zig:253-259
        Some(layer) => {
            if sync {
                layer.set_surface_sync(&target.surface);
            } else {
                // try self.layer.setSurface(target.surface);  (Metal.zig:257)
                layer.set_surface(&target.surface)
                    .map_err(|iosurface_layer::Error::ObjCFailed| Error::ObjCFailed)?;
            }
            Ok(())
        }
        // OFFSCREEN (layer None): no layer to set; the completion path reads back via
        // Target::read_bgra instead. Preserved no-op (never exercised on-screen).
        None => Ok(()),
    }
}
```
`iosurface_layer::Error::ObjCFailed` folds into `Error::ObjCFailed` (identity — faithful to ghostty's `try setSurface` propagating `error{ObjCFailed}` into the `!void` that `generic.rs`'s `drawFrame` error set already threads). `target.surface` is `CFRetained<IOSurfaceRef>`; `&target.surface` deref-coerces to `&IOSurfaceRef`.

### `present_last_target` (654-656) — unchanged (`_ = self;` no-op for Metal; keep the R1 comment).

### `loop_enter` (383-387; verbatim comment 371-382) — **DEFER to W2, keep `unimplemented!`**
`loopEnter` (Metal.zig:164-175) recovers `renderer` via `@fieldParentPtr("api", self)` and installs `displayCallback` → `renderer.drawFrame(true)`. The trampoline needs `GenericRenderer` in scope (container_of over `api`), which lives in `generic.rs`, and `drawFrame(true)` is only driven by the render **thread** (W2). Per the plan W1-3 port-note ("If W1 scopes loop_enter narrowly, leave it `unimplemented!()` and land the trampoline in W2"), **W1 leaves `loop_enter` as the `unimplemented!("windowing-host: …")` seam** and records the split in `DEFERRALS.md`. The `set_display_callback` primitive it calls is already W0-landed; only the trampoline authoring waits for the W2 render thread. This keeps W1 present-only and avoids importing an unused display-callback path.

**KAT** (`onscreen_surface_size_and_present_from_layer`, macOS): probe-skip; `let view: Retained<AnyObject> = unsafe { msg_send![class!(NSView), new] }`; `let mut m = Metal::init(&*view,1.0,Native)?` (**`let mut`** — `present` takes `&mut self`); `layer.as_ca_layer().setBounds(CGRect(0,0,48,24))` + `setContentsScale(1.0)`; assert `surface_size() == (48,24)`; then draw a real IOSurface Target offscreen (`off = init_offscreen(Native,48,24); target = off.init_target(48,24)`), `m.present(&target, true)`, and assert `let c = unsafe { m.layer…as_ca_layer().contents() };` (**`contents()` is `unsafe fn`**) pointer-equals `&*target.surface`. Keep `off` alive past the assert.

**Risk:** Med (accessor names — now VERIFIED above). `loop_enter` deferral must be flagged so a completeness critic doesn't expect it filled in W1.

---

## W1-4 — `Frame::complete` async `addCompletedHandler` present half

**File:** `crates/rustty-renderer/src/metal/frame.rs`.

**Edits:**
1. **`CompletionContext` (46-54)** — add `#[derive(Clone, Copy)]`. All three fields (`*mut ()`, `*mut Target`, `bool`) are `Copy`; the RcBlock must own a **by-value copy** (mirrors ghostty's `CompletionBlock.init(.{…})` copying the `Context` into the block, so the async body does not borrow `self.block` which would dangle after `complete` returns). Document this on the derive.
2. **`complete` (185-199)** — replace the `unimplemented!("windowing-host: async addCompletedHandler present")` at 198 with the block install (the R5 shape comment 186-197 becomes the implementation, verbatim):
   ```rust
   if !sync {
       // Copy the context by value + the fn-ptr by move (matches Zig's by-value
       // Context copy — stays valid until Metal fires the handler). The block is
       // copied when addCompletedHandler: retains it and dealloc'd by the objc
       // runtime on completion (Frame.zig:106-108).
       let ctx = self.block;               // Copy
       let on_completed = self.on_completed;
       let handler: block2::RcBlock<dyn Fn(core::ptr::NonNull<ProtocolObject<dyn MTLCommandBuffer>>)> =
           block2::RcBlock::new(move |buf: core::ptr::NonNull<ProtocolObject<dyn MTLCommandBuffer>>| {
               // SAFETY: buf is a valid MTLCommandBuffer for the callback's duration;
               // ctx's raw ptrs are derefed NEVER as &mut (same container_of discipline
               // as the sync path / metal_buffer_completed).
               unsafe { on_completed(&ctx, buf.as_ref()); }
           });
       unsafe { self.buffer.addCompletedHandler(block2::RcBlock::as_ptr(&handler)); }
   }
   self.buffer.commit();
   // ... sync branch unchanged (205-218: waitUntilCompleted; block.sync=true;
   //     (self.on_completed)(&self.block, &self.buffer)) ...
   ```
3. **Module doc (26)** — drop the "(`unimplemented!`)" note on the async path; the sync path remains "needs NO ObjC block".

`block2`/`objc2-metal` `addCompletedHandler` (feature `block2`) are already deps. `RcBlock<dyn Fn(NonNull<…>)>` matches `MTLCommandBufferHandler` (the frame.rs:192-197 comment already pinned this shape).

**KAT** (`frame_complete_async_installs_handler_and_commits`, macOS): probe-skip; `begin_frame(null, &mut target, on_done)`; `frame.complete(false)`; `frame.buffer.waitUntilCompleted()`; bounded spin (≤1000 `yield_now`) on a `static AtomicBool FIRED` set by `on_done`. Assert fired.

**Risk:** Med — async fire is timing-dependent; if flaky on CI, downgrade to asserting `complete(false)` returns without panic and defer fire-observation to W2's render-thread integration (flag in the KAT comment). Miri-excluded (foreign calls).

---

## W1-5 — restore the R1 present-failure error log in `metal_buffer_completed`

**File:** `crates/rustty-renderer/src/generic.rs` (4618-4632).

Now that `present` can fail on the layer path (W1-3), the `let _ = …present(…)` at line 4628 must **handle** the now-fallible `Result` faithfully. **BLOCKER RESOLVED (build-critical):** `log` is **NOT** a dependency of `rustty-renderer` (verified: `crates/rustty-renderer/Cargo.toml` `[dependencies]` has no `log`; EVERY `log.*` call site tree-wide — `shadertoy.rs`, `metal_impl.rs:72`, `buffer.rs`, `pipeline.rs`, `target.rs`, `generic.rs:2254`, and the existing 4620-4627 block itself — is preserved as a **comment** precisely because "no logging facade is wired in the port yet"). Emitting `log::error!` would break `cargo build` (unresolved crate) AND contradict this design's own "No Cargo.toml change" claim. The faithful R1 encoding keeps the `log.err` present-failure line as the already-present comment and threads the fallible `Result` without any facade. Replace line 4628's `let _ = unsafe { (*renderer).api.present(&*ctx.target, ctx.sync) };` with:
```rust
// block.renderer.api.present(block.target.*, block.sync) catch |err|
//     log.err("Failed to present render target: err={}", .{err});   (Frame.zig:82-88)
// No logging facade is wired in the port yet (see the tree-wide `log.*`-as-comment
// convention); the present-failure `log.err` stays the R1 comment above. We still
// consume the now-fallible Result faithfully (the layer path can fail; W1-3).
if let Err(_e) = unsafe { (*renderer).api.present(&*ctx.target, ctx.sync) } {
    // log.err("Failed to present render target: err={}", .{_e}); — no facade
}
```
No Cargo.toml change; no `log::` call is emitted. The `frame_completed(health)` call (4631) is unchanged. The R1 comment block 4620-4627 collapses into the live code (keep the Frame.zig line cite).

**KAT** (`buffer_completed_offscreen_still_healthy`, in the generic test mod): `golden_renderer(6,2)` → drive `"Hi"` → `update_frame` → `draw_frame(true)`; assert `r.health.load(SeqCst) == Health::Healthy` (offscreen present is the `None` no-op ⇒ no `Err` ⇒ no spurious log; `frame_completed` still fired). The real on-screen present-error path is covered by W1-6.

**Risk:** Low — one-line verbatim un-defer.

**W1-5 vs W1-7 conflict (RESOLVE HERE):** W1-5's port-note claims "`Renderer::init`/`deinit` STAY `unimplemented!()` in W1". This note predates the amendment that **added W1-7**. The **Canonical Interfaces** (authoritative) list `Renderer::init`/`deinit` as *FILLED by W1*, and W1-7 is the task that fills them. **Resolution:** W1-7 fills `init`/`deinit`; W1-5's "stay unimplemented" clause is stale and is superseded. W1-5 itself still only edits `metal_buffer_completed` (its scope is unchanged). Record the resolution in `DEFERRALS.md`.

---

## W1-6 — CULMINATING KAT: on-screen present of one golden-state frame (FIRST PIXELS)

**File:** `crates/rustty-renderer/src/generic.rs` — add `golden_presents_frame_on_screen` INSIDE the existing `#[cfg(test)] mod` (beside `golden_renders_cursor_fg_bg`, ~after 5950) so it reaches the private `golden_renderer`/`px_at` helpers + `Renderer.swap_chain` internals (an external `tests/` file cannot — many fields are `pub(crate)`).

**Two-phase, headless-proven** structure (the plan's robust form):
1. **Draw** one real terminal frame into an IOSurface Target via the proven offscreen golden path: `golden_renderer(10,3)` (probe-skips), stream `"Ab"`, `update_frame(&st,true)`, `draw_frame(true)`. `idx = swap_chain.frame_index`.
2. **Confirm real pixels** — reuse the offscreen golden assert: `px_at(&px, w, 2*cw+cw/2, ch/2)` is red-dominant (`rr>128 && rr>g && rr>b`) ⇒ genuine terminal pixels, not blank.
3. **Present on-screen** — `let view: Retained<AnyObject> = unsafe { msg_send![class!(NSView), new] }`; `let mut onscreen = Metal::init(&*view, 1.0, Native)?` (**`&*view`** deref-coerce; **`let mut`** — `present` is `&mut self`); `onscreen.present(&r.swap_chain.frames[idx].target, true)` ⇒ `set_surface_sync` ⇒ `CALayer.setContents(surface)`. **FIRST PIXELS.**
4. **Assert the handshake** — `let c = unsafe { onscreen.layer…as_ca_layer().contents() };` (**`contents()` is `unsafe fn`**) pointer-equals `&*…frames[idx].target.surface`; and `unsafe { msg_send![&*view, layer] }` pointer-equals `onscreen.layer…as_ca_layer()` (view is layer-hosting our layer). `t.deinit(Global)`.

This inherits pixel parity from the offscreen golden gate (same `px_at` assert), then adds the on-screen delta (present → layer contents). It deliberately draws with the OFFSCREEN api (`golden_renderer` pins `offscreen_size == size.screen`, avoiding a `draw_frame` surface_size resize when a layer's bounds differ) and presents through a SEPARATE on-screen `Metal` — the cleanest faithful proof of the `setSurface` primitive against real terminal pixels. The visible NSWindow is W4, not here.

SB/TB: `present` takes `&Target` from `r.swap_chain` (shared) while `r` is not mutably borrowed across the call — no `&mut` aliasing.

**Fallback (plan-sanctioned):** if `contents()` returns a wrapped/copied object rather than the exact `IOSurfaceRef` pointer, weaken the identity assert to `contents.is_some()` + a `read_bgra` pixel re-check — still proves first pixels. (The W0 `iosurface_layer_sets_surface_as_contents` KAT already established pointer-identity holds, so the strong assert is expected to pass.)

**Risk:** Med — depends on W1-1..W1-3 + W0. Bare-NSView on headless CI (see W1-2 mitigation).

---

## W1-7 — Renderer windowed lifecycle: fill `init`/`deinit` + `surface_init`/`finalize_surface_init`

**File:** `crates/rustty-renderer/src/generic.rs` (macos_impl module). Fills the seams at 1793 (`init`) and 1840 (`deinit`); adds the two surface hooks.

### The apprt-free surface handshake — DESIGN DECISION

The **Canonical Interface** pins the signatures with a **bare, unbounded `<S>`**:
```rust
pub fn surface_init<S>(rt_surface: *mut S) -> Result<(), Error>;
pub fn finalize_surface_init<S>(&mut self, rt_surface: *mut S) -> Result<(), Error>;
```
An unbounded `<S>` cannot dereference/message-send `rt_surface`. This is not an oversight — it is **exactly faithful**: ghostty's `surfaceInit` (generic.zig:864-869) and `finalizeSurfaceInit` (873-878) are `@hasDecl(GraphicsAPI, "surfaceInit"/"finalizeSurfaceInit")` probes, and **`Metal.zig` declares NEITHER hook** (it declares only `loopEnter`). So for the Metal monomorphization **both are no-ops**:
```rust
/// generic.zig:864-869 surfaceInit. `@hasDecl(GraphicsAPI, "surfaceInit")` is FALSE
/// for Metal ⇒ the probe's else-arm ⇒ no-op. `rt_surface` is the apprt surface the
/// hook is generic over (never named by rustty-renderer, R4); Metal ignores it.
pub fn surface_init<S>(rt_surface: *mut S) -> Result<(), Error> {
    // if (@hasDecl(GraphicsAPI, "surfaceInit")) try GraphicsAPI.surfaceInit(surface);
    let _ = rt_surface;
    Ok(())
}
/// generic.zig:873-878 finalizeSurfaceInit. `@hasDecl(GraphicsAPI,
/// "finalizeSurfaceInit")` is FALSE for Metal ⇒ no-op.
pub fn finalize_surface_init<S>(&mut self, rt_surface: *mut S) -> Result<(), Error> {
    // if (@hasDecl(GraphicsAPI, "finalizeSurfaceInit")) try self.api.finalizeSurfaceInit(surface);
    let _ = rt_surface;
    Ok(())
}
```
Both preserve the reference `@hasDecl` body as the comment (R1). The generic `<S>` keeps rustty-renderer from naming any apprt type (R4) while matching the `renderer_seam` placeholder signatures.

**Where does the on-screen layer attach happen, then?** In ghostty the attach is 100% inside `Metal.init` via `opts.rt_surface`. The port defers `rt_surface` out of `Options` (R4 — `surface_mailbox`/`rt_surface`/`thread` stay in `renderer_seam::Options<A>` in core). So the attach is performed by **`Metal::init` (W1-2) called directly by the windowing host / `renderer_seam` wiring (W3)** — not inside `Renderer::init`, which has no surface. This is the documented, forced relocation; it is proven end-to-end in W1-6 (present onto an `Metal::init` view). `surface_init`/`finalize_surface_init` stay faithful no-ops.

### `Renderer::init(alloc, options)` (fill 1793; verbatim body comment 1650-1792)

Port the reference `init` body (generic.zig:658-798) verbatim, with **one sanctioned, apprt-forced divergence**: the graphics-API construction uses the offscreen-portable device ctor instead of the live-surface `GraphicsAPI.init`, because `Options` carries no surface. This is the "reuse `init_offscreen`'s shared body, do NOT duplicate" directive at the Renderer level:
```rust
pub fn init(alloc: Global, options: Options) -> Result<Self, Error> {
    // var api = try GraphicsAPI.init(alloc, options);   (generic.zig:662)
    // DIVERGENCE (R4/apprt-free): Options carries no rt_surface (surface stays in
    // renderer_seam::Options<A>), so `api` is built via the offscreen-portable device
    // ctor (layer None). The on-screen layer attach is supplied by the windowing host
    // via Metal::init (W1-2) at the renderer_seam wiring (W3) — behaviorally identical
    // to ghostty across the init -> host-attach sequence.
    let api = Metal::init_offscreen(
        options.config.blending,
        options.size.screen.width,
        options.size.screen.height,
    )?;                                                   // errdefer api.deinit() -> Drop

    let has_custom_shaders = !options.config.custom_shaders.value.is_empty(); // generic.zig:665

    let swap_chain = SwapChain::init(&api, has_custom_shaders)?;  // errdefer -> Drop
    let font_shaper = Shaper::init(shape::Options {
        features: options.config.font_features.clone(),          // generic.zig:675-677
        ..Default::default()
    })?;                                                          // errdefer -> Drop

    // font_critical: grid.lock.lockShared(); defer unlockShared(); metrics = grid.metrics
    //   (generic.zig:681-690). The shared-read lock is LOAD-BEARING and MUST be mirrored
    //   (R1): the reference takes `grid.lock.lockShared()` before reading `grid.metrics`.
    //   `SharedGrid` exposes `pub lock: RwLock<()>` with `.read()` (shared_grid.rs:89/346
    //   precedent — every `lockShared` site in the port is `self.lock.read().unwrap()`).
    let grid_metrics = {
        // const grid: *font.SharedGrid = options.font_grid;   (generic.zig:684)
        let g: *mut SharedGrid = options.font_grid;
        // grid.lock.lockShared(); defer grid.lock.unlockShared();   (685-686)
        let _read = unsafe { (*g).lock.read().unwrap() };
        // break :font_critical .{ .metrics = grid.metrics };   (687-689)
        unsafe { (*g).metrics }
    };

    // display_link: vsync ? DisplayLink.createWithActiveCGDisplays() : null (692-698)
    // W1: display link is a W2 concern -> None (the .macos vsync arm lands in W2).
    let display_link: Option<DisplayLink> = None;

    // R1/COMPILE-COMPLETENESS: Rust has NO struct-field defaults, so the literal must
    // enumerate EVERY field of the real `Renderer` struct — not just the subset the
    // reference literal (701-787) spells out. The reference OMITS the fields that are
    // Zig struct-declaration defaults; the port must map each to that decl default,
    // EXACTLY mirroring the existing `golden_renderer` (5769-5810)/`test_renderer`
    // literal (which is the authoritative full field set). The full field list:
    let mut result = Self {
        // ---- generic.zig:701-787 (spelled out in the reference literal) ----
        alloc,                                       // 702
        config: options.config,                      // 703
        surface_mailbox: SurfaceMailbox,             // 704 (options.surface_mailbox deferred to renderer_seam; R4)
        grid_metrics,                                // 705 (font_critical.metrics)
        size: options.size,                          // 706
        focused: true,                               // 707
        scrollbar: Scrollbar::ZERO,                  // 708 (.zero)
        scrollbar_dirty: false,                      // 709
        last_bottom_node: None,                      // 710
        last_bottom_y: 0,                            // 711
        search_matches: None,                        // 712
        search_selected_match: None,                 // 713
        search_matches_dirty: false,                 // 714
        cells,                                        // 717 (Contents, sized to GridSize{rows,cols} above; .{} + resize)
        uniforms: unsafe { core::mem::zeroed() },    // 718-743 (projection/cell/grid/screen `undefined` → zeroed POD,
                                                     //   fixed by update_*_uniforms below; the min_contrast/cursor_pos
                                                     //   maxInt/bg_color/bools live fields are re-applied by those calls
                                                     //   — mirror golden_renderer's `zeroed()` then update, faithful to
                                                     //   Zig's `undefined` + `update*Uniforms`)
        custom_shader_uniforms: unsafe { core::mem::zeroed() }, // 744-772 (the literal is all zero/defaults; zeroed POD)
        bg_image_buffer: unsafe { core::mem::zeroed() },        // 773 (undefined)
        font_grid: options.font_grid,                // 776
        font_shaper,                                  // 777
        font_shaper_cache: ShaperCache::init(),      // 778
        shaders,                                      // 781 (undefined → set by init_shaders below; see NB)
        api,                                          // 784
        swap_chain,                                   // 785
        display_link,                                 // 786 (None in W1; W2 vsync arm)
        // ---- Zig struct-decl defaults (OMITTED by 701-787; enumerate per golden_renderer) ----
        draw_mutex: Mutex::new(),                    // struct-decl default
        custom_shader_focused_changed: false,        // struct-decl default
        cells_rebuilt: false,                        // struct-decl default
        first_frame_time: None,                      // struct-decl default
        last_frame_time: None,                        // struct-decl default
        images: image::State::empty(),               // struct-decl default (.{} empty)
        bg_image: None,                               // struct-decl default
        bg_image_changed: false,                      // struct-decl default
        bg_image_buffer_modified: 0,                 // struct-decl default
        health: AtomicHealth::new(crate::Health::Healthy), // struct-decl default (.healthy)
        target_config_modified: 0,                   // struct-decl default
        reinitialize_shaders: false,                 // struct-decl default
        has_custom_shaders,                          // = options.config.custom_shaders != empty (computed above, 665)
        terminal_state: RenderState::empty(),        // struct-decl default
        terminal_state_frame_count: 0,               // struct-decl default
        overlay: None,                                // struct-decl default
    };

    result.init_shaders()?;                    // try result.initShaders();  (789)
    result.update_font_grid_uniforms();        // (792)
    result.update_screen_size_uniforms();      // (793)
    result.update_bg_image_buffer();           // (794)
    result.prep_background_image()?;            // try result.prepBackgroundImage(); (795)
    Ok(result)
}
```
The literal is `golden_renderer`/`test_renderer` field-for-field (that harness pair IS the authoritative full field set — cross-check both when authoring, since 701-787 alone is incomplete). `shaders` starts from `init_shaders()` (mirror the golden harness, which binds `let shaders = api.init_shaders(&[])?` BEFORE the literal, then the `result.init_shaders()?` here re-runs the reference's `.shaders = undefined; result.initShaders()`). **NB (`has_custom_shaders`):** unlike the golden harness (which hard-codes `false`), `init` must set it from the computed `options.config.custom_shaders != empty` (generic.zig:665) so `init_shaders`/`draw_frame` see the real value. All `update_*`/`prep` calls already exist and are exercised by the golden tests.

**De-duplication (fulfils "do NOT duplicate") — DELEGATE `golden_renderer` ONLY, NOT `test_renderer`:** the test helpers `golden_renderer` (5679) and `test_renderer` (5543) currently hand-build the identical field literal + `update_*_uniforms`.

**`golden_renderer` (SAFE to delegate):** it PINS `offscreen_size == size.screen` (§3.2: `init_offscreen(blending, size.screen.width, size.screen.height)`) — exactly what `Renderer::init` does. Refactor it to build `Options { config: derived_config_for_test(), font_grid, size }` (after the font-grid/size setup it already does) and delegate to `Renderer::init(Global, opts)`. Compatibility: `init` sets `offscreen_size = size.screen` — identical to the golden pin; `update_*_uniforms` — identical; `prep_background_image` with `config.bg_image = None` — no-op; `update_bg_image_buffer` — harmless; `has_custom_shaders` — `derived_config_for_test()` has no custom shaders ⇒ `false`, matching the hand-built value. The T3 bg-image test's manual `prep_background_image` after config mutation is unaffected.

**`test_renderer` (do NOT delegate — CORRECTNESS):** `test_renderer` (5545) DELIBERATELY pins its offscreen device to a FIXED `Metal::init_offscreen(Native, 256, 256)` — **unequal** to `size.screen` — whereas `Renderer::init` builds `api` via `init_offscreen(blending, size.screen.width, size.screen.height)`. Delegating would change `surface_size()` from 256×256 to the grid pixel size for the ~9 `test_renderer` callers (6259-6545), FLIPPING `draw_frame`'s `size_changed` path (256×256 ≠ grid-size ⇒ `size_changed == true`) and silently altering those tests' semantics. Only `golden_renderer` (offscreen == size.screen) was analyzed compatible. **KEEP `test_renderer` hand-built** with its explicit `init_offscreen(Native, 256, 256)`. The remaining shared field-fill (should a critic still want the ~100-line duplication removed) is factored via a shared private `fn assemble(api, shaders, swap_chain, font_grid, grid_metrics, size, cells, config) -> Renderer` that BOTH `test_renderer` (256×256 api) and `Renderer::init` (size.screen api) call — the API/offscreen-size difference stays at each caller, the literal is written once. Delegation of `golden_renderer` to `Renderer::init` remains the primary de-dup; `test_renderer` is explicitly out of scope for delegation.

### `Renderer::deinit(&mut self)` (fill 1840; verbatim body comment 1808-1839)

Port generic.zig:800-830 verbatim. Most fields are `Drop`-managed in Rust; call the explicit `deinit`s where the ported types expose them, and comment each Zig line that maps to a Drop:
```rust
pub fn deinit(&mut self) {
    if let Some(overlay) = &mut self.overlay { overlay.deinit(self.alloc); }   // 801
    self.terminal_state.deinit(self.alloc);                                    // 802
    if let Some(m) = &mut self.search_selected_match { m.arena.deinit(); }     // 803
    if let Some(m) = &mut self.search_matches { m.arena.deinit(); }            // 804
    self.swap_chain.deinit();                                                  // 805
    // DisplayLink teardown (807-812): guarded until W2 lands the display link.
    // if (DisplayLink != void) { if (self.display_link) |dl| { dl.stop(); dl.release(); } }
    if let Some(_dl) = &self.display_link { /* W2: dl.stop(); dl.release() (ARC) */ }
    self.cells.deinit(self.alloc);                                             // 814
    self.font_shaper.deinit();                                                 // 816
    self.font_shaper_cache.deinit();                                           // 817 (no alloc arg — cache.rs:112)
    self.config.deinit();                                                      // 819
    self.images.deinit(self.alloc);                                            // 821
    if let Some(img) = &mut self.bg_image { img.deinit(self.alloc); }          // 823
    self.deinit_shaders();                                                     // 825 (pub fn, generic.rs:4647)
    // self.api.deinit();  (827) -> Metal Drop (queue/device/layer ARC). Explicit
    //   drop is unnecessary; documented (the `&mut self` deinit cannot move `api`).
    // self.* = undefined;  (829) -> no-op (Rust drop/ownership handles teardown).
}
```
Whichever of `terminal_state`/`cells`/`images`/`font_shaper`/`config` expose a manual `deinit(alloc)` in the port, call it faithfully (they do — the golden tests and `mark_dirty`/`set_font_grid` already call `font_shaper_cache.deinit()`/`ShaperCache`). Where a ported type has NO manual `deinit` (pure Drop), preserve the Zig line as a comment noting the Drop. **The verbatim body comment 1808-1839 must survive** (`check_comment_coverage.py`).

**KAT** (`windowed_lifecycle` mod, `#[cfg(all(test, target_os="macos"))]`): `windowed_renderer_init_then_deinit_round_trips` — build `Options { config: derived_config_for_test(), font_grid: <leaked test grid>, size }` (reuse `golden_renderer`'s grid/size setup, or a small `test_support` helper); probe-skip if no Metal device; `Renderer::surface_init(view_ptr)` (no-op ok); `let mut r = Renderer::init(Global, opts)?`; `r.finalize_surface_init(view_ptr)` (no-op ok); `r.deinit()` must not panic/leak (ASan covers UB across the objc FFI). The `view_ptr` can be any `*mut S` (the hooks ignore it); to keep the KAT honest about the generic, pass a real bare-NSView `*mut AnyObject`.

**Risk:** Med — large faithful body; mitigated by reusing `init_offscreen` (shared device prologue) + delegating the golden harness. Execute the design→adversary workflow against `generic.zig` init/deinit line-by-line.

---

## Cross-cutting: gates, deferrals, files touched

**Files modified (all `rustty-renderer`):**
- `src/metal/metal_impl.rs` — W1-1 (field), W1-2 (`init` + `init_device` helper), W1-3 (`surface_size`/`present` + new `Error::ObjCFailed` variant; `loop_enter` stays deferred).
- `src/metal/frame.rs` — W1-4 (`CompletionContext: Copy` + async block).
- `src/generic.rs` — W1-5 (`metal_buffer_completed` log), W1-6 (culminating KAT), W1-7 (`init`/`deinit`/`surface_init`/`finalize_surface_init`; refactor `golden_renderer`/`test_renderer` to delegate).
- **No Cargo.toml change** (objc2-quartz-core `CALayer`/`CAMetalLayer` features already in-lock from W0; no app-kit dep).

**`DEFERRALS.md` entries (W1):**
1. `Metal::loop_enter` stays `unimplemented!()` — the `displayCallback` trampoline needs `GenericRenderer` in scope + the W2 render thread; lands in W2.
2. `Renderer::deinit` DisplayLink teardown arm guarded — the `.macos` vsync display-link lands in W2.
3. `Renderer::init` builds `api` via `init_offscreen` (layer None) — the on-screen layer attach is supplied by the host via `Metal::init` (W1-2) at the W3 renderer_seam wiring; `surface_init`/`finalize_surface_init` are faithful no-ops (Metal declares neither `@hasDecl` hook).
4. Supersede W1-5's stale "init/deinit stay unimplemented in W1" note — W1-7 fills them (Canonical authoritative).
5. **End-to-end completion→present chain onto the renderer's OWN layer is NOT exercised in W1 (scope, not a defect).** In W1 `Renderer::init` ALWAYS builds `api` via `init_offscreen` (layer `None` ⇒ `present` is the no-op `None` arm; the `metal_buffer_completed` present call at generic.rs:4628 never reaches `set_surface`). W1-6 proves "first pixels" by calling `present` **directly** on a SEPARATE on-screen `Metal::init` instance against a real golden-rendered Target — NOT via the renderer's completion handler onto its own attached layer. The full route `draw_frame → Frame.complete (async addCompletedHandler) → metal_buffer_completed → present → IOSurfaceLayer.set_surface` onto a REAL on-screen layer is wired in **W3** (renderer_seam host attach), when `Renderer` gains an on-screen `api`. A completeness critic must NOT expect the end-to-end completion→present→set_surface chain in W1.

**`MIRI_EXCEPTIONS.md`:** add `metal_impl::tests::{offscreen_metal_has_no_layer, init_binds_layer_to_nsview, onscreen_surface_size_and_present_from_layer}`, `metal::frame::tests::frame_complete_async_installs_handler_and_commits`, `generic::tests::{buffer_completed_offscreen_still_healthy, golden_presents_frame_on_screen}`, `generic::windowed_lifecycle::windowed_renderer_init_then_deinit_round_trips` (all FFI/objc2 — foreign calls, ASan-eligible per R6).

**Gates (union across tasks):** `cargo build -p rustty-renderer` · `cargo test -p rustty-renderer --lib` (offscreen golden + all W1 KATs green) · `cargo clippy -p rustty-renderer --all-targets -- -D warnings` · `cargo fmt --check` · `bash .port/tools/gates/check_acyclic.sh` (no app-kit/apprt edge) · `python3 .port/tools/gates/check_comment_coverage.py` (verbatim init/deinit bodies survive) · `python3 .port/tools/gates/check_test_counts.py`.

**Disposition: ready (fold pass complete).** The one build-BLOCKER (W1-5 emitting `log::error!` when `log` is not a renderer dep) is RESOLVED: the present-failure `log.err` stays the R1 comment and the now-fallible `Result` is consumed with a facade-free `if let Err(_e) = … { /* no facade */ }` — no Cargo.toml change. All six adversary fixes folded in: (1) W1-7 `init` mirrors the load-bearing `grid.lock.read()` shared-read critical section; (2) `test_renderer` is explicitly NOT delegated to `Renderer::init` (its 256×256 offscreen pin ≠ `size.screen` would flip `size_changed`) — only `golden_renderer` delegates; (3) the `init` struct literal now enumerates EVERY field (Zig-decl defaults included) mirroring `golden_renderer`/`test_renderer`; (4) KAT compile-invariants (`contents()` is `unsafe fn`; `present` needs `let mut`; `[NSView new]` → `&*view` coercion; `msg_send!` unsafe) are pinned globally + in each KAT; (5) the present error fold uses a dedicated faithful `Error::ObjCFailed` (ghostty's `setSurface` error set) instead of mislabeling as `NoMetalDevice`; (6) the never-exercised-in-W1 end-to-end completion→present→set_surface chain is stated explicitly in DEFERRALS (lands W3). The surface-handshake apprt tension remains resolved by the faithful `@hasDecl`-no-op reading of the bare-`<S>` Canonical signatures + the documented `Metal::init` attach relocation.
