//! Tests for `generic::macos_impl` (ghostty src/renderer/generic.zig, the
//! `Renderer(Metal)` arm) — the offscreen golden renders (cursor/fg/bg, on-screen
//! present, background image, custom shader, kitty image) and the layer display
//! callback.

use super::*;

// Sample the BGRA byte 4-tuple at pixel (x, y) in a tight `w*h*4` readback
// (`read_bgra` returns `[B,G,R,A]` per pixel, rows tight). Returns (b,g,r,a).
fn px_at(px: &[u8], w: usize, x: usize, y: usize) -> (u8, u8, u8, u8) {
    let i = (y * w + x) * 4;
    (px[i], px[i + 1], px[i + 2], px[i + 3])
}

// Encode a `w`×`h` solid-RGB PNG for the bg-image golden (T3) — the reference
// `background-image` fixture is not checked in, so generate it at test time via
// the pure-Rust `png` encoder (test-helper duplication precedent — `opts`).
fn encode_solid_png(w: u32, h: u32, rgb: [u8; 3]) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc.write_header().expect("png header");
        let mut data = Vec::with_capacity((w * h * 3) as usize);
        for _ in 0..(w * h) {
            data.extend_from_slice(&rgb);
        }
        writer.write_image_data(&data).expect("png data");
        writer.finish().expect("png finish");
    }
    out
}

// GPU_RD_CHUNK4_DESIGN §3 — the R-d contract's FIRST real GPU-output assertion:
// render a deterministic terminal state OFFSCREEN (Metal → IOSurface), read the
// pixels back, and assert the STRUCTURE the port is responsible for (§3.1: not an
// exact-PNG gate — that would be flaky across FreeType/metal-compiler versions —
// but structural asserts with thresholds lenient enough to survive the P3
// conversion / premultiply / BGRA8Unorm rounding, yet non-vacuous). macOS +
// `#[cfg(test)]`; Metal-skip on a headless host.
//
// DETERMINISM PINS (§3.2 / OPEN RISK #5, load-bearing): `AlphaBlending::Native`
// (⇒ `BGRA8Unorm`, no store-time sRGB re-encode), `colorspace = Srgb` (⇒
// `use_display_p3 = false`), and `offscreen_size == size.screen` (⇒
// `size_changed == false`). If a future change flips blending to linear the
// target becomes `_sRGB` and the readback semantics shift.
// W3 §5 (bounded-pump Surface::draw KAT). `Surface::draw` (core) forwards
// one line to `drawFrame(true)` -> the real `draw_frame(true)` ("Renderers
// are required to support `drawFrame` being called from the main thread",
// Surface.zig:884). A full core `Surface::draw` unit test needs the whole
// apprt mock + a live windowing host (the windowing-integration milestone),
// so this bounded N-frame pump lands at the renderer's offscreen harness and
// proves the main-thread-present leg is safe to pump repeatedly + leaves the
// renderer Healthy.
//
// Coverage boundary: this does NOT drive the `threadMain` event loop or any
// callback-driven draw — that path is proven by the xev-stubbed mirihar
// `renderthread_shipped` candidate + `cargo build` type-checking the real
// `threadMain`/callbacks against xev/renderer signatures.
#[test]
fn draw_frame_bounded_pump() {
    let mut r = match golden_renderer(10, 3) {
        Some(r) => r,
        None => return, // Metal-skip
    };
    // bounded — never a live loop.
    for _ in 0..8 {
        r.draw_frame(true).expect("main-thread draw_frame(true)");
        assert_eq!(r.health.load(), crate::Health::Healthy);
    }
    // deinit path clean.
    r.thread_exit();
    r.deinit();
}

#[test]
fn golden_renders_cursor_fg_bg() {
    let mut r = match golden_renderer(10, 3) {
        Some(r) => r,
        None => return, // Metal-skip
    };

    // "Ab" on a 10×3 grid ⇒ a block cursor at grid (2,0) (the empty cell after
    // "Ab"; focused + blink_visible ⇒ block). Verified by the 3b test.
    let mut mutex = Mutex::new();
    let mut t = Terminal::init(Global.into(), opts(10, 3)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(b"Ab");
        s.deinit();
    }
    let st = state::State {
        mutex: &mut mutex,
        terminal: &mut t,
        inspector: None,
        inspector_ptr: None,
        preedit: None,
        mouse: state::Mouse::default(),
        // D4 handoff (oracle d34b54e9b) - reference field defaults are `.init(0)`.
        demand: Default::default(),
        handoff_gen: Default::default(),
        handoff_park: Default::default(),
    };
    // Drive the full pipeline through update_frame (3c): fills cells + uniforms;
    // rebuild_cells sets dirty. cursor_blink_visible=true ⇒ visible block cursor.
    r.update_frame(&st, true).unwrap();
    // sync ⇒ commit + waitUntilCompleted + the completion trampoline runs.
    r.draw_frame(true).unwrap();

    // Read back the IOSurface pixels of the frame just drawn (all fields pub;
    // read_bgra handles bytesPerRow).
    let idx = r.swap_chain.as_ref().unwrap().frame_index as usize;
    let px = r.swap_chain.as_ref().unwrap().frames[idx]
        .target
        .read_bgra();

    let w = r.size.screen.width as usize;
    let cw = r.grid_metrics.cell_width as usize;
    let ch = r.grid_metrics.cell_height as usize;
    assert_eq!(px.len(), w * (r.size.screen.height as usize) * 4);

    // (A) CURSOR CELL — solid red block at grid (2,0). Sample the cell interior
    //     center. Can only pass if `cell_text` ran with `is_cursor_glyph` set +
    //     `cursor_color` red reached the shader ⇒ proves the packed `bools` byte
    //     (bit 1) AND the uniform serialization.
    let (b, g, rr, _a) = px_at(&px, w, 2 * cw + cw / 2, ch / 2);
    assert!(rr > 128, "cursor cell not bright-red (R={rr})");
    assert!(
        rr > g && rr > b,
        "cursor cell not red-dominant (R={rr} G={g} B={b})"
    );

    // (B) BACKGROUND — config bg is black (0,0,0), opacity 1.0 ⇒ bg_color =
    //     [0,0,0,255]. Sample an empty cell far from glyphs/cursor (grid (8,2),
    //     bottom row): cell_bg is transparent there (alpha 0) ⇒ the opaque
    //     full-screen bg_color layer shows through. The RGB check ALONE is
    //     VACUOUS here (config bg RGB == clear RGB == 0; bg_color_fragment
    //     premultiplies rgb*a ⇒ also RGB 0), so we ALSO assert the ALPHA: the
    //     bg_color step writes an opaque layer (a=255) whereas the render-pass
    //     clear leaves a=0 ⇒ `ba > 128` can only pass if the full-screen bg_color
    //     step ran AND `bg_color.a = 255` serialized (byte 131 = offset 128 + 3).
    let (bb, bg2, br, ba) = px_at(&px, w, 8 * cw + cw / 2, 2 * ch + ch / 2);
    assert!(
        br < 96 && bg2 < 96 && bb < 96,
        "background not dark (R={br} G={bg2} B={bb})"
    );
    assert!(
        ba > 128,
        "bg_color step absent / bg alpha not opaque (A={ba})"
    );

    // (C) FOREGROUND GLYPH present in row 0 — "A" at grid (0,0) is white. Scan
    //     the cell for a bright mark (antialiased glyph ⇒ max luminance over the
    //     cell). Proves the atlas sync + `cell_text` glyph sampling.
    let mut maxlum = 0u32;
    for yy in 0..ch {
        for xx in 0..cw {
            let (b3, g3, r3, _) = px_at(&px, w, xx, yy);
            maxlum = maxlum.max((r3 as u32 + g3 as u32 + b3 as u32) / 3);
        }
    }
    assert!(
        maxlum > 128,
        "no fg glyph rendered in cell (0,0) (maxlum={maxlum})"
    );

    t.deinit(Global.into());
}

// W5 HEADLESS KAT (`W5_DESIGN §5`): prove the display-callback trampoline
// end-to-end with NO window server / live render thread. Build a `Renderer`
// with a real (headless) `IOSurfaceLayer`, run the REAL `Metal::loop_enter`
// (container_of + `set_display_callback`), fire the layer's ObjC `display`
// method directly (exactly the message Core Animation sends on the main thread
// during a `needsDisplayOnBoundsChange` commit), and assert a full sync frame
// ran through `display_callback` → `draw_frame_raw`.
//
// Single-threaded, so it proves the FUNCTIONAL leg only — it exercises ZERO
// cross-thread aliasing. The concurrency soundness (§2.5's raw-self re-route)
// is covered SOLELY by the `mirihar-chunk3b` `displaycb_concurrent`/
// `displaycb_naive` candidates (§6); neither artifact substitutes for the other.
#[test]
fn loop_enter_display_callback_draws_a_frame() {
    use crate::metal::iosurface_layer::IOSurfaceLayer;
    use objc2::msg_send;
    use objc2_core_foundation::{CGPoint, CGRect, CGSize};

    let mut r = match golden_renderer(10, 3) {
        Some(r) => r,
        None => return, // Metal-skip
    };

    // Give the terminal some content so the draw is non-trivial (mirrors golden).
    let mut mutex = Mutex::new();
    let mut t = Terminal::init(Global.into(), opts(10, 3)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(b"Ab");
        s.deinit();
    }
    let st = state::State {
        mutex: &mut mutex,
        terminal: &mut t,
        inspector: None,
        inspector_ptr: None,
        preedit: None,
        mouse: state::Mouse::default(),
        // D4 handoff (oracle d34b54e9b) - reference field defaults are `.init(0)`.
        demand: Default::default(),
        handoff_gen: Default::default(),
        handoff_park: Default::default(),
    };
    r.update_frame(&st, true).unwrap();

    // Inject a headless IOSurfaceLayer (offscreen ctor leaves layer = None). All
    // fields are pub. `IOSurfaceLayer::init()` is window-server-free (W0 tests).
    let layer = IOSurfaceLayer::init().expect("IOSurfaceLayer::init");
    // Size it so surface_size == size.screen (scale 1 ⇒ size_changed == false ⇒
    // the frame draws the existing cells rather than taking the resize path).
    layer.as_ca_layer().setContentsScale(1.0);
    layer.as_ca_layer().setBounds(CGRect::new(
        CGPoint::new(0.0, 0.0),
        CGSize::new(r.size.screen.width as f64, r.size.screen.height as f64),
    ));
    r.api.set_layer_for_test(layer);

    // Install the display callback via the REAL seam (container_of +
    // set_display_callback). `loop_enter` needs a whole-`Renderer`-provenance
    // `*mut Metal` (§2.4).
    let self_ptr: *mut Renderer = &mut r;
    let api_ptr: *mut Metal = unsafe { &raw mut (*self_ptr).api };
    let lock = unsafe { (*self_ptr).draw_mutex.clone() };
    unsafe { Metal::loop_enter(api_ptr, lock) };

    // Arm an observable: draw_frame(true) drives the sync completion trampoline,
    // which sets health = Healthy. Pre-set Unhealthy so a no-op / early-return
    // fails loudly.
    r.health.store(crate::Health::Unhealthy);

    // Fire the ObjC `display` method — exactly what Core Animation sends on the
    // main thread during a needsDisplayOnBoundsChange commit. This drives
    // display_callback → draw_frame_raw(self_ptr, true).
    let disp_layer = r.api.layer().unwrap();
    let _: () = unsafe { msg_send![&*disp_layer.layer, display] };

    // The trampoline ran a full sync frame: completion set health = Healthy …
    assert_eq!(
        r.health.load(),
        crate::Health::Healthy,
        "display → display_callback → draw_frame_raw must run a full frame"
    );
    // … and present(sync) set the layer contents to the drawn IOSurface.
    let contents: Option<objc2::rc::Retained<objc2::runtime::AnyObject>> =
        unsafe { r.api.layer().unwrap().layer.contents() };
    assert!(
        contents.is_some(),
        "sync present must set the layer contents"
    );

    r.thread_exit();
    r.deinit();
    t.deinit(Global.into());
}

// P7-W7 T5b fix round 2 (ruling 46): `Renderer::deinit` must clear the layer's
// display callback (the reference's `Metal.deinit` → `layer.release()`,
// upstream 4b4a5b241), because the host view can keep the layer alive after
// the renderer is freed and a later Core Animation display pass would run the
// callback on it. Before the fix only `Metal`'s `Drop` cleared it, and the
// core surface's owner never runs that `Drop`.
#[test]
fn deinit_clears_the_layer_display_callback() {
    use crate::metal::iosurface_layer::IOSurfaceLayer;
    use objc2::msg_send;
    use objc2_core_foundation::{CGPoint, CGRect, CGSize};

    let mut r = match golden_renderer(10, 3) {
        Some(r) => r,
        None => return, // Metal-skip
    };
    let layer = IOSurfaceLayer::init().expect("IOSurfaceLayer::init");
    layer.as_ca_layer().setContentsScale(1.0);
    layer.as_ca_layer().setBounds(CGRect::new(
        CGPoint::new(0.0, 0.0),
        CGSize::new(r.size.screen.width as f64, r.size.screen.height as f64),
    ));
    r.api.set_layer_for_test(layer);
    let self_ptr: *mut Renderer = &mut r;
    let api_ptr: *mut Metal = unsafe { &raw mut (*self_ptr).api };
    let lock = unsafe { (*self_ptr).draw_mutex.clone() };
    unsafe { Metal::loop_enter(api_ptr, lock) };
    assert!(
        r.api.has_display_ctx(),
        "loop_enter installs the callback context"
    );

    // The host view's retention of the backing CALayer, outliving the renderer.
    let host_retained = r.api.layer().unwrap().layer.clone();

    r.thread_exit();
    r.deinit();

    assert!(
        !r.api.has_display_ctx(),
        "deinit must free the display-callback context"
    );
    // A later display pass on the host-retained layer must not reach the
    // renderer: with the callback still installed it would run a sync draw,
    // whose completion sets health to Healthy.
    r.health.store(crate::Health::Unhealthy);
    let _: () = unsafe { msg_send![&*host_retained, display] };
    assert_eq!(
        r.health.load(),
        crate::Health::Unhealthy,
        "a display pass after deinit must not run the renderer's callback"
    );
    // The owner then drops the renderer (core `RendererBox::dealloc_after_deinit`);
    // `Metal::drop` re-runs its now-idempotent `deinit`.
    drop(r);
}

// A minimal ObjC test-double for the host NSView. `Metal::init` (the on-screen
// constructor) attaches its `IOSurfaceLayer` by sending the layer-hosting
// selectors `setLayer:` / `setWantsLayer:` / `setClipsToBounds:` to a
// type-erased `&AnyObject` (R4 — never names an app-kit type), then the W1-6
// handshake reads `[view layer]` back. This double responds to EXACTLY those
// four selectors — storing the `setLayer:` id and returning it from `layer` —
// so `Metal::init`'s present-path msg_sends run byte-identically to a real
// NSView, while the object is a PLAIN `NSObject` subclass (define_class!, the
// W0 `IOSurfaceLayer` precedent).
//
// WHY a double instead of a real `[NSView new]` (which the W1 design sketched):
// the lib test binary links only QuartzCore + Metal — `NSView`/AppKit is NOT
// loaded (R4 forbids the `objc2-app-kit` dep), so `class!(NSView)` would panic;
// and even after `dlopen`-ing AppKit, `NSView` mutation (`setWantsLayer:` …)
// enforces MAIN-THREAD use and THROWS an ObjC exception on cargo's parallel
// test worker threads (an uncatchable foreign exception → `SIGABRT`), which no
// `dispatch_sync` can rescue (the harness main thread runs no runloop). A plain
// `NSObject` subclass has no AppKit main-thread constraint, so the KAT is robust
// under the default parallel `cargo test` — the faithful headless test vehicle
// (the visible NSWindow is W4). It exercises the SAME `Metal::init` code path;
// only the receiver of the (side-effect-free-for-Metal) attach selectors is the
// double rather than a live view.
use objc2::DefinedClass as _;
struct MockHostViewIvars {
    layer: core::cell::Cell<*mut objc2::runtime::AnyObject>,
}
objc2::define_class!(
    // SAFETY: NSObject has no subclassing requirements; no Drop impl.
    #[unsafe(super(objc2::runtime::NSObject))]
    #[name = "RusttyMockHostView"]
    #[ivars = MockHostViewIvars]
    struct MockHostView;

    impl MockHostView {
        // `[view setLayer: l]` — store the CALayer id (arrives as an object ptr).
        #[unsafe(method(setLayer:))]
        fn set_layer(&self, layer: *mut objc2::runtime::AnyObject) {
            self.ivars().layer.set(layer);
        }
        // `[view layer]` — return the stored layer (the W1-6 handshake reads it).
        #[unsafe(method(layer))]
        fn layer(&self) -> *mut objc2::runtime::AnyObject {
            self.ivars().layer.get()
        }
        // The remaining two attach selectors are no-ops for a layer double.
        #[unsafe(method(setWantsLayer:))]
        fn set_wants_layer(&self, _wants: bool) {}
        #[unsafe(method(setClipsToBounds:))]
        fn set_clips_to_bounds(&self, _clips: bool) {}
    }
);

// W1-6 CULMINATING KAT — FIRST PIXELS on-screen. Render one real terminal
// frame OFFSCREEN (the proven golden path), then present it through the REAL
// windowed present path: `Metal::init(view)` (the on-screen constructor —
// creates an `IOSurfaceLayer`, attaches it to the host view as a layer-hosting
// view) → `present(target, sync)` → `IOSurfaceLayer::set_surface_sync` →
// `CALayer.setContents(surface)`. Then assert (a) the drawn pixels are genuine
// terminal red (inherits the offscreen golden gate's pixel parity via the same
// `px_at` cursor-cell probe), (b) the layer's `contents` IS the exact IOSurface
// we drew (FIRST PIXELS), and (c) the view hosts our layer. The visible
// NSWindow is W4 — this KAT is HEADLESS-friendly (no WindowServer / run loop):
// the host view is a plain-`NSObject` layer-host double (see `MockHostView`).
//
// R4/headless: `rustty-renderer` MUST NOT depend on `objc2-app-kit` (no
// compile-time app-kit edge, `check_acyclic.sh`). The `MockHostView` double
// needs no AppKit at all — it responds to the four layer-host selectors
// `Metal::init` sends, and is main-thread-safe under parallel `cargo test`.
// Skips first on no-Metal (via `golden_renderer`). FFI/objc2 ⇒ Miri-excluded,
// ASan-eligible (R6). SB/TB: `present` takes `&Target` from `r.swap_chain`
// (shared) while `r` is not mutably borrowed across the call — no `&mut` alias.
#[test]
fn golden_presents_frame_on_screen() {
    use core::ffi::c_void;

    use objc2::msg_send;
    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2::AnyThread;
    use objc2_io_surface::IOSurfaceRef;
    use objc2_quartz_core::CALayer;

    // --- (1) DRAW one real terminal frame OFFSCREEN (the proven golden path).
    //         Probe-skips on a host with no Metal device.
    let mut r = match golden_renderer(10, 3) {
        Some(r) => r,
        None => return, // Metal-skip
    };

    // "Ab" on a 10×3 grid ⇒ a block cursor at grid (2,0) (the empty cell after
    // "Ab") — identical to the offscreen cursor golden.
    let mut mutex = Mutex::new();
    let mut t = Terminal::init(Global.into(), opts(10, 3)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(b"Ab");
        s.deinit();
    }
    let st = state::State {
        mutex: &mut mutex,
        terminal: &mut t,
        inspector: None,
        inspector_ptr: None,
        preedit: None,
        mouse: state::Mouse::default(),
        // D4 handoff (oracle d34b54e9b) - reference field defaults are `.init(0)`.
        demand: Default::default(),
        handoff_gen: Default::default(),
        handoff_park: Default::default(),
    };
    r.update_frame(&st, true).unwrap();
    r.draw_frame(true).unwrap();

    let idx = r.swap_chain.as_ref().unwrap().frame_index as usize;

    // --- (2) CONFIRM real pixels (reuse the offscreen golden assert). The
    //         cursor cell (grid (2,0)) interior center is a solid red block ⇒
    //         genuine terminal pixels, not a blank/uninitialized surface.
    let px = r.swap_chain.as_ref().unwrap().frames[idx]
        .target
        .read_bgra();
    let w = r.size.screen.width as usize;
    let cw = r.grid_metrics.cell_width as usize;
    let ch = r.grid_metrics.cell_height as usize;
    let (b, g, rr, _a) = px_at(&px, w, 2 * cw + cw / 2, ch / 2);
    assert!(
        rr > 128 && rr > g && rr > b,
        "no real terminal pixels to present (R={rr} G={g} B={b})"
    );

    // --- (3) PRESENT on-screen via the REAL windowed path. FIRST PIXELS.
    // Build the layer-host double (a plain NSObject subclass — no AppKit).
    let view: Retained<MockHostView> = {
        let this = MockHostView::alloc().set_ivars(MockHostViewIvars {
            layer: core::cell::Cell::new(core::ptr::null_mut()),
        });
        unsafe { msg_send![super(this), init] }
    };
    // `&*view` (`&MockHostView`) deref-coerces to `&AnyObject` (via
    // MockHostView → NSObject → AnyObject) at the `Metal::init` call site.
    let view_any: &AnyObject = &view;

    // `present` takes `&self` (N440).
    let onscreen = Metal::init(view_any, 1.0, AlphaBlending::Native)
        .expect("Metal::init (Metal probed above)");

    // present(sync=true) → set_surface_sync → CALayer.setContents(surface).
    onscreen
        .present(&r.swap_chain.as_ref().unwrap().frames[idx].target, true)
        .expect("present");

    // --- (4) ASSERT the handshake.
    let layer = onscreen.layer().expect("on-screen Metal has a layer");
    // (a) the layer's `contents` IS the exact IOSurface we drew. `contents()`
    //     is an `unsafe fn` (objc2-quartz-core `CALayer.rs`).
    let contents: Retained<AnyObject> =
        unsafe { layer.as_ca_layer().contents() }.expect("layer contents set by present");
    let want_surface = (&*r.swap_chain.as_ref().unwrap().frames[idx].target.surface
        as *const IOSurfaceRef)
        .cast::<c_void>();
    let got_surface = Retained::as_ptr(&contents) as *const c_void;
    assert_eq!(
        got_surface, want_surface,
        "layer contents must be the presented IOSurface (FIRST PIXELS)"
    );
    // (b) the view hosts our layer (Metal::init's setLayer: stored it in the
    //     double; `[view layer]` returns that stored id).
    let hosted: *mut AnyObject = unsafe { msg_send![&*view, layer] };
    let want_layer = (layer.as_ca_layer() as *const CALayer).cast::<c_void>();
    assert_eq!(
        hosted as *const c_void, want_layer,
        "view must host our IOSurfaceLayer"
    );

    t.deinit(Global.into());
}

// T3 (RENDERER_BG_IMAGE_DESIGN §4) — the offscreen proof that the whole
// background-image pipeline executes end-to-end: point the config at a real
// PNG on disk, `prep_background_image` (open + FileType sniff + wuffs decode ->
// `bg_image = Some(Pending)`), `update_bg_image_buffer` (opacity/fit ->
// `BgImageGpu`), `draw_frame` (`upload_background_image` -> `Ready(texture)` +
// the full-screen bg_image render step), then read back the IOSurface and
// assert the sampled pixels are the DECODED image color — distinct from the
// plain `bg_color` (black) the negative control shows. macOS/Metal-gated;
// Metal-skip on a headless host (same pattern as the cursor golden).
//
// DETERMINISM PINS (mirroring the cursor golden): `AlphaBlending::Native`
// (BGRA8Unorm, no store-time sRGB re-encode), `colorspace = Srgb`,
// `offscreen == size.screen`. `fit = Stretch` so the solid image fills the
// whole screen ⇒ every pixel is the image color (trivial sampling).
#[test]
fn golden_renders_background_image() {
    use std::ffi::CString;

    // The distinct bg-image color (blue-dominant), unambiguously != black.
    const IMG_RGB: [u8; 3] = [0x30, 0x60, 0xC0];

    // Helper: build a golden renderer, optionally with a bg image at `path`,
    // draw one frame, and return the center-pixel BGRA of the bottom row.
    fn draw_center(with_bg: Option<&std::path::Path>) -> Option<(u8, u8, u8, u8)> {
        let mut r = golden_renderer(10, 3)?;

        if let Some(p) = with_bg {
            // Point the config at the on-disk PNG (required arm) + stretch it.
            let bytes = p.as_os_str().as_encoded_bytes().to_vec();
            r.config.bg_image = Some(crate::generic::Path::Required(CString::new(bytes).unwrap()));
            r.config.bg_image_fit = BackgroundImageFit::Stretch;
            r.config.bg_image_opacity = 1.0;
            // Decode + build the vertex/opacity buffer (the two calls
            // `change_config` would make on a bg-image change).
            r.prep_background_image().expect("prep_background_image");
            r.update_bg_image_buffer();
            // The decode must have produced a pending bg image.
            assert!(
                matches!(r.bg_image, Some(image::Image::Pending(_))),
                "prep did not stage a pending bg image"
            );
        }

        // An (empty) terminal drives update_frame/draw_frame (as the cursor
        // golden does); the bg-image step does not depend on cell content.
        let mut mutex = Mutex::new();
        let mut t = Terminal::init(Global.into(), opts(10, 3)).unwrap();
        let st = state::State {
            mutex: &mut mutex,
            terminal: &mut t,
            inspector: None,
            inspector_ptr: None,
            preedit: None,
            mouse: state::Mouse::default(),
            // D4 handoff (oracle d34b54e9b) - reference field defaults are `.init(0)`.
            demand: Default::default(),
            handoff_gen: Default::default(),
            handoff_park: Default::default(),
        };
        r.update_frame(&st, true).unwrap();
        r.draw_frame(true).unwrap();

        if with_bg.is_some() {
            // After the frame, the pending image was uploaded to the GPU.
            assert!(
                matches!(r.bg_image, Some(image::Image::Ready(_))),
                "bg image not uploaded to a Ready texture"
            );
        }

        let idx = r.swap_chain.as_ref().unwrap().frame_index as usize;
        let px = r.swap_chain.as_ref().unwrap().frames[idx]
            .target
            .read_bgra();
        let w = r.size.screen.width as usize;
        let cw = r.grid_metrics.cell_width as usize;
        let ch = r.grid_metrics.cell_height as usize;
        // Sample the center of an empty cell on the bottom row (grid (5,2)).
        let out = px_at(&px, w, 5 * cw + cw / 2, 2 * ch + ch / 2);
        t.deinit(Global.into());
        Some(out)
    }

    // Write the solid bg-image PNG to a temp file.
    let png_bytes = encode_solid_png(2, 2, IMG_RGB);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("rustty_bg_{nanos}.png"));
    std::fs::write(&path, &png_bytes).unwrap();

    // (bg image) — sample should be the decoded image color.
    let bg = match draw_center(Some(&path)) {
        Some(v) => v,
        None => {
            let _ = std::fs::remove_file(&path);
            return; // Metal-skip
        }
    };
    // (negative control) — no bg image ⇒ the plain black bg_color.
    let none = draw_center(None).expect("Metal probed above");
    let _ = std::fs::remove_file(&path);

    // read_bgra returns (B, G, R, A). The image is blue-dominant (0x30,0x60,0xC0):
    //   B ≈ 0xC0 (>128), R ≈ 0x30 (<128), B > R, and the layer is opaque.
    let (bb, _bg2, br, ba) = bg;
    assert!(bb > 128, "bg-image blue channel too low (B={bb})");
    assert!(br < 128, "bg-image red channel too high (R={br})");
    assert!(bb > br, "bg-image not blue-dominant (B={bb} R={br})");
    assert!(ba > 128, "bg-image layer not opaque (A={ba})");

    // The negative control is dark (black bg_color) — proves the bg-image step
    // (not the bg_color fallback) produced the color above.
    let (nb, ng, nr, _na) = none;
    assert!(
        nb < 96 && ng < 96 && nr < 96,
        "negative control not dark (B={nb} G={ng} R={nr})"
    );
    assert!(
        bb > nb + 64,
        "bg-image blue not distinct from bg_color (bg B={bb} vs none B={nb})"
    );
}

// RENDERER_CUSTOM_SHADERS_DESIGN §4.2 — the offscreen end-to-end proof that the
// custom-shader (shadertoy) post-process path executes. Point the config at an
// on-disk pass-through shader, run `init_shaders` (load + transpile MSL + build
// the post pipelines + flip `has_custom_shaders`), then drive one frame with
// visible text. The Chunk-4 `draw_frame` passes (retarget the cell pass to
// `back_texture`, then the ping-pong post loop blitting back to `frame.target`)
// run for the FIRST time here. Assert the per-frame `CustomShaderState` was
// created, the post pipelines were built, and the pass-through preserved the
// cells (non-black AND not a uniform fill — a black frame is the empty-
// `post_pipelines` failure mode in OPEN RISK R1). macOS/Metal-gated; Metal-skip.
//
// Assertion strategy (§4.2): the "recognizable" contract is the robustness
// check — a non-black pixel AND a non-uniform frame — NOT an exact baseline
// diff (the pass-through samples `texture(iChannel0, gl_FragCoord/iResolution)`
// through a Metal sampler, so sub-LSB filtering can perturb an exact match while
// leaving the image recognizable). Both halves still fail the real regressions:
// an empty-pipeline black frame fails non-black; a uniform fill fails non-uniform.
#[test]
fn golden_renders_custom_shader() {
    use std::ffi::CString;
    let mut r = match golden_renderer(10, 3) {
        Some(r) => r,
        None => return, // Metal-skip
    };

    // Write the pass-through shadertoy body to a nanos-suffixed temp file
    // (mirrors the bg-image golden).
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("rustty_shader_{nanos}.glsl"));
    std::fs::write(
        &path,
        b"void mainImage(out vec4 fragColor, in vec2 fragCoord){\n\
          fragColor = texture(iChannel0, fragCoord / iResolution.xy);\n}\n",
    )
    .unwrap();

    // Point the config at the on-disk pass-through shader (required arm),
    // byte-faithful path -> CString (mirrors the bg-image golden).
    let cpath = CString::new(path.as_os_str().as_encoded_bytes().to_vec()).unwrap();
    r.config.custom_shaders = RepeatablePath {
        value: vec![crate::generic::Path::Required(cpath)],
    };

    // Load + transpile + build the post pipelines + flip has_custom_shaders.
    unsafe { Renderer::init_shaders_raw(&mut r) }.expect("init_shaders");
    assert!(r.has_custom_shaders, "custom shaders not activated");
    assert!(
        !r.shaders.post_pipelines.is_empty(),
        "post pipelines not built"
    );

    // Drive one frame with a non-empty terminal (write visible text so the cell
    // pass produces a recognizable, non-black image the pass-through preserves).
    let mut mutex = Mutex::new();
    let mut t = Terminal::init(Global.into(), opts(10, 3)).unwrap();
    t.print_string(b"HELLO").unwrap();
    let st = state::State {
        mutex: &mut mutex,
        terminal: &mut t,
        inspector: None,
        inspector_ptr: None,
        preedit: None,
        mouse: state::Mouse::default(),
        // D4 handoff (oracle d34b54e9b) - reference field defaults are `.init(0)`.
        demand: Default::default(),
        handoff_gen: Default::default(),
        handoff_park: Default::default(),
    };
    r.update_frame(&st, true).unwrap();
    r.draw_frame(true).unwrap();

    // The draw_frame branch lazily created the per-frame CustomShaderState.
    let idx = r.swap_chain.as_ref().unwrap().frame_index as usize;
    assert!(
        r.swap_chain.as_ref().unwrap().frames[idx]
            .custom_shader_state
            .is_some(),
        "custom_shader_state not created by draw_frame"
    );

    // Pass-through = identity: the cells survive to frame.target.
    let px = r.swap_chain.as_ref().unwrap().frames[idx]
        .target
        .read_bgra();
    // Non-black: some pixel has a bright channel (the white "HELLO" glyphs).
    let non_black = px
        .chunks_exact(4)
        .any(|p| p[0] > 64 || p[1] > 64 || p[2] > 64);
    assert!(
        non_black,
        "cells not visible through the pass-through shader"
    );
    // Not uniform: at least one pixel differs from the first (glyph vs bg).
    let first = &px[0..4];
    let non_uniform = px.chunks_exact(4).any(|p| p != first);
    assert!(
        non_uniform,
        "frame is a uniform fill (post-process did not preserve the image)"
    );

    t.deinit(Global.into());
    let _ = std::fs::remove_file(&path);
}

// RENDERER_KITTY_IMAGE_PATH_DESIGN §4 — the offscreen APC-driven proof that
// the 5 filled Kitty pixel-path seams (`kitty_update` +
// `prep_kitty_placement`/`prep_kitty_virtual_placement`/`prep_kitty_image` +
// `Image::convert`) execute end-to-end: drive a real `Terminal` with a
// Kitty-graphics transmit+display APC, run `update_frame` (which enters
// `kitty_update` via the `kitty_requires_update` guard) then `draw_frame`
// (which uploads pending images to the GPU), and assert the STATE the path
// produces. macOS/Metal-gated; Metal-skip like the golden.
#[test]
fn kitty_image_pixel_path() {
    let mut r = match golden_renderer(10, 3) {
        Some(r) => r,
        None => return, // Metal-skip
    };

    // Transmit AND display (a=T) a 1x2 direct RGB image (f=24), id 1, all
    // pixels 0xFF (`////////` = 6 bytes of 0xFF). a=T places it at the
    // cursor → a pin placement, marking storage dirty.
    let mut mutex = Mutex::new();
    let mut t = Terminal::init(Global.into(), opts(10, 3)).unwrap();
    // The vt `Terminal` starts with `width_px`/`height_px == 0` (the port
    // sets them only through the surface). Kitty `pixel_size` divides by the
    // cell size (`width_px / cols`), so pin the pixel dims to the renderer's
    // cell metrics — exactly what a real surface resize would do — else the
    // placement's dest size is 0 and it is (correctly) culled.
    t.width_px = r.grid_metrics.cell_width * 10;
    t.height_px = r.grid_metrics.cell_height * 3;
    {
        let mut s = t.vt_stream();
        s.next_slice(b"\x1b_Ga=T,f=24,i=1,s=1,v=2,c=1,r=1;////////\x1b\\");
        s.deinit();
    }

    // Sanity: the image + a placement now live in storage, dirty is set.
    let storage = unsafe { &(*t.screens.active).kitty_images };
    assert!(storage.image_by_id(1).is_some(), "image not in storage");
    assert!(!storage.placements.is_empty(), "no placement in storage");
    assert!(storage.dirty, "storage not marked dirty by the APC");

    // Drive the renderer. `terminal_state.update` snapshots the terminal;
    // `update_frame` → `kitty_requires_update` (dirty) == true → `kitty_update`.
    r.terminal_state.update(Global.into(), &mut t).unwrap();
    let st = state::State {
        mutex: &mut mutex,
        terminal: &mut t,
        inspector: None,
        inspector_ptr: None,
        preedit: None,
        mouse: state::Mouse::default(),
        // D4 handoff (oracle d34b54e9b) - reference field defaults are `.init(0)`.
        demand: Default::default(),
        handoff_gen: Default::default(),
        handoff_park: Default::default(),
    };
    r.update_frame(&st, true).unwrap();

    // (a) the terminal-walk + containment math ran → a placement was computed.
    assert!(
        !r.images.kitty_placements.is_empty(),
        "kitty_update computed no placements"
    );
    // (b) `prep_kitty_image` queued the image (pending) or it is already up.
    let entry = r
        .images
        .images
        .get(&crate::image::Id::Kitty(1))
        .expect("no Id::Kitty(1) image entry after kitty_update");
    assert!(
        entry.image.has_texture() || entry.image.is_pending(),
        "image neither pending nor uploaded after kitty_update"
    );
    // (c) `kitty_update`'s tail cleared `storage.dirty` (through the raw ptr).
    assert!(
        !unsafe { &(*t.screens.active).kitty_images }.dirty,
        "kitty_update did not clear storage.dirty"
    );

    // `draw_frame` uploads pending images → `convert` (rgb→rgba) +
    // `Texture::init` run → the image is Ready with a texture.
    r.draw_frame(true).unwrap();
    assert!(
        r.images
            .images
            .get(&crate::image::Id::Kitty(1))
            .expect("image entry vanished")
            .image
            .has_texture(),
        "image has no GPU texture after draw_frame (convert/upload did not run)"
    );

    t.deinit(Global.into());
}
