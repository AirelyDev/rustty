#![cfg(target_os = "macos")]
#![allow(non_snake_case)] // PARITY: reference C-ABI identifiers verbatim.
//! W4 culminating KAT — FIRST TIME the port hosts a macOS layer-host view and
//! presents an on-screen terminal frame through the published C-ABI with NO Swift
//! app. A Rust-owned layer-host view (a plain-NSObject `MockHostView` double, the
//! renderer `golden_presents_frame_on_screen` precedent — main-thread-agnostic, no
//! AppKit) is handed across the C-ABI (platform.macos.nsview); the embedded
//! Surface's LIVE Platform/content-scale handshake (E2/E3) drives the W1-W3
//! renderer to attach an IOSurfaceLayer to that view and present an IOSurface; a
//! bounded live-loop pump (live_shell.rs discipline) ticks until FIRST PIXELS, then
//! the presented IOSurface is read back and structurally compared to the committed
//! offscreen golden gate. display_id + blur are exercised end-to-end. The real
//! NSWindow lives ONLY in the #[ignore] on-device test.
//!
//! Real objc/Metal/CGS FFI + a live xev render loop => Miri-excluded
//! (.port/phase-2/MIRI_EXCEPTIONS.md, "rustty-apprt-c W4"). The `MockHostView`
//! double has no AppKit main-thread affinity, so the headless tests run correctly
//! on libtest's parallel worker threads (unlike a real NSWindow, which would need
//! the process main thread libtest does not provide).
//!
//! W4 DEVIATION — REVISED 2026-07-10 (VALIDATION_REPORT M4 re-run on a Metal host):
//! the ORIGINAL deviation blamed `Metal::loop_enter` being `unimplemented!()`; that
//! leaf LANDED in W5 (metal_impl.rs `loop_enter` is live), so the old reason is
//! stale. The M4 re-run found the TRUE state:
//! - `set_display_id_pushes_to_render_thread_panic_free` PASSES on a Metal host
//!   (run 2026-07-10, this machine) — it is UN-IGNORED below.
//! - `hosts_view_layer_and_presents_live_frame` is now GREEN and UN-IGNORED
//!   (RESOLVED P0.1, 2026-07-12). W-ATTACH (commit 96b0116) wired the on-screen
//!   host-attach — `rustty_surface_new` → `Renderer::finalizeSurfaceInit` →
//!   `Metal::attach_view` sends `setLayer:` to the host view, so the FIRST assertion
//!   ("host view is layer-hosting") passes. The remaining red was purely a
//!   headless-harness gap (NOT product): the inert MockHostView never lays its
//!   hosted layer out, so `surfaceSize()` (= `layer.bounds * contentsScale`) read
//!   0×0 and `drawFrame` early-returned; and the render thread's async
//!   `present(sync=false)` hop lands on the main dispatch queue that no headless
//!   runloop drains. `pump_bounded` now faithfully stands in for AppKit + the CA
//!   commit (sizes the layer + drives `-display`, the reference sync present path);
//!   see the RESOLVED note in `.port/phase-2/DEFERRALS.md` "W-ATTACH".
//!   ALSO FOUND + HANDLED (2026-07-10, lldb): two
//!   tests calling `rustty_app_new` on parallel libtest workers SIGABRT with zero
//!   diagnostics inside HIToolbox (`Keymap::init` ->
//!   `TISCopyCurrentKeyboardLayoutInputSource` -> abort; TIS is not thread-safe) —
//!   app creation in this binary is serialized via `app_new_serialized` to restore
//!   the mirrored one-main-thread host contract.
//!
//! The W4-2 blur sub-KAT does NOT spawn a surface/render thread, so it runs GREEN
//! headless and proves the realized blur leaf (real `windowNumber` msg_send + the
//! private CGS SPI) end-to-end.
use std::ffi::{c_char, c_int, c_void};
use std::time::{Duration, Instant};

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject};
use objc2::{define_class, msg_send, AnyThread, DefinedClass};

// The concrete embedded backend types the C-ABI crosses. `TargetC`/`ActionC` live
// in `rustty_apprt_api::action` (the `action` callback's params), and the two
// `Clipboard*` request types in `rustty_apprt_api` (the callback-table params); the
// CAPI extern `ClipboardContent` is the one defined IN `embedded`.
use rustty_apprt::embedded::{
    App, ClipboardContent, Options, PlatformC, PlatformCMacOS, Surface, SurfaceOptions,
};
use rustty_apprt_api::action::{ActionC, TargetC};
use rustty_apprt_api::{ClipboardRequest, ClipboardRequestType};
use rustty_apprt_c::capi::Text;
use rustty_apprt_c::{
    rustty_app_free, rustty_app_new, rustty_app_tick, rustty_app_update_config, rustty_init,
    rustty_set_window_background_blur, rustty_surface_config_new, rustty_surface_free,
    rustty_surface_free_text, rustty_surface_ime_point, rustty_surface_mouse_scroll,
    rustty_surface_new, rustty_surface_quicklook_font, rustty_surface_quicklook_word,
    rustty_surface_set_content_scale, rustty_surface_set_display_id, rustty_surface_set_size,
};
use rustty_config::value_types::BackgroundBlur;

// The C host contract requires `rustty_init` (the `ghostty_init` global-state
// entrypoint, lib.rs:150) BEFORE any `rustty_app_new` — it seeds `rustty_core::STATE`.
// `rustty_apprt::init()` sets a process-wide `OnceLock`, so drive it EXACTLY once
// across the (parallel) tests in this binary.
fn ensure_init() {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        // SAFETY: the current init scaffold does not dereference argv (argc=0).
        let rc = unsafe { rustty_init(0, core::ptr::null_mut()) };
        assert_eq!(rc, 0, "rustty_init");
    });
}

// M4 diagnosis (2026-07-10, lldb): two libtest worker threads calling
// `rustty_app_new` CONCURRENTLY abort inside Apple's HIToolbox —
// `App::init` -> `Keymap::init` -> `TISCopyCurrentKeyboardLayoutInputSource` ->
// `islGetInputSourceListWithAdditions` -> `abort()` (TIS is not thread-safe).
// The C host contract this KAT mirrors calls `ghostty_app_new` from the main
// thread (one at a time); libtest's parallel workers violate that contract, so
// app CREATION is serialized here — test-harness glue restoring the mirrored
// calling convention, NOT a port change (the
// `parallel-cargo-test-exposes-c-init-races` precedent).
static APP_NEW_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// All `rustty_app_new` calls in this binary go through here.
///
/// # Safety
/// Same contract as `rustty_app_new`: `opts`/`config` live for the call.
unsafe fn app_new_serialized(opts: &Options, config: *mut rustty_config::Config) -> *mut App {
    let _guard = APP_NEW_LOCK.lock().unwrap();
    // SAFETY: forwarded caller contract (see doc above).
    unsafe { rustty_app_new(opts, config) }
}

// --- MockHostView: the layer-host double handed across the C-ABI as `nsview` ---
// A plain `NSObject` subclass (the renderer `generic.rs:6531` precedent, verbatim
// selectors) so it has NO AppKit main-thread constraint and needs NO
// `MainThreadMarker`. It responds to exactly the selectors the port sends it:
//   - `setLayer:`/`layer`/`setWantsLayer:`/`setClipsToBounds:` — `Metal::init`'s
//     layer-host handshake (stores the CALayer id, returns it from `layer`);
//   - `windowNumber` — the W4-2 blur leaf's `msg_send![window, windowNumber]`
//     (the headless blur test passes THIS double as the `window` arg; a fixed
//     small positive id keeps the private-CGS call well-formed).
struct MockHostViewIvars {
    layer: core::cell::Cell<*mut AnyObject>,
}
define_class!(
    // SAFETY: NSObject has no subclassing requirements; no Drop impl.
    #[unsafe(super(NSObject))]
    #[name = "RusttyApprtCMockHostView"]
    #[ivars = MockHostViewIvars]
    struct MockHostView;

    impl MockHostView {
        // `[view setLayer: l]` — store the CALayer id (arrives as an object ptr).
        #[unsafe(method(setLayer:))]
        fn set_layer(&self, layer: *mut AnyObject) {
            self.ivars().layer.set(layer);
        }
        // `[view layer]` — return the stored layer (the handshake reads it back).
        #[unsafe(method(layer))]
        fn layer(&self) -> *mut AnyObject {
            self.ivars().layer.get()
        }
        // The remaining two attach selectors are no-ops for a layer double.
        #[unsafe(method(setWantsLayer:))]
        fn set_wants_layer(&self, _wants: bool) {}
        #[unsafe(method(setClipsToBounds:))]
        fn set_clips_to_bounds(&self, _clips: bool) {}
        // `[window windowNumber]` — NSInteger; a stable small positive id.
        #[unsafe(method(windowNumber))]
        fn window_number(&self) -> isize {
            1
        }
    }
);
fn make_host_view() -> Retained<MockHostView> {
    let this = MockHostView::alloc().set_ivars(MockHostViewIvars {
        layer: core::cell::Cell::new(core::ptr::null_mut()),
    });
    // SAFETY: `super(this), init` initializes the freshly-allocated NSObject.
    unsafe { msg_send![super(this), init] }
}

// --- minimal C-ABI runtime callback table (embedded::Options) ---
extern "C" fn cb_wakeup(_ud: *mut c_void) {}
// improper_ctypes_definitions: `ActionC`/`TargetC` transitively carry `rustty_input`'s
// `Mods` (not yet byte-exact-C — the DOCUMENTED apprt-api deferral); silenced exactly
// as the production `embedded::Options` callback table silences it.
#[allow(improper_ctypes_definitions)]
extern "C" fn cb_action(_app: *mut App, _t: TargetC, _a: ActionC) -> bool {
    true
}
extern "C" fn cb_read_clipboard(
    _ud: *mut c_void,
    _t: c_int,
    _r: *mut ClipboardRequest,
    _mimes: *const *const c_char,
    _mimes_len: usize,
    _list: bool,
) -> c_int {
    // GHOSTTY_CLIPBOARD_READ_UNSUPPORTED — the request is never started.
    2
}
extern "C" fn cb_confirm_read_clipboard(
    _ud: *mut c_void,
    _s: *const rustty_apprt::embedded::ClipboardConfirm,
    _r: *mut ClipboardRequest,
    _t: ClipboardRequestType,
) {
}
extern "C" fn cb_write_clipboard(
    _ud: *mut c_void,
    _t: c_int,
    _c: *const ClipboardContent,
    _n: usize,
    _cf: bool,
) {
}
extern "C" fn cb_close_surface(_ud: *mut c_void, _p: bool) {}

fn test_options() -> Options {
    Options {
        userdata: core::ptr::null_mut(),
        supports_selection_clipboard: false,
        wakeup: cb_wakeup,
        action: cb_action,
        read_clipboard: cb_read_clipboard,
        confirm_read_clipboard: cb_confirm_read_clipboard,
        write_clipboard: cb_write_clipboard,
        close_surface: Some(cb_close_surface),
    }
}

// rustty_config_new() (config c_api.rs:29) then _finalize; fields are pub, so the
// opacity/blur overrides are direct writes before the finalize commits.
fn rustty_config_new_and_finalize() -> *mut rustty_config::Config {
    let cfg = rustty_config::c_api::rustty_config_new();
    // SAFETY: `cfg` is a freshly-allocated live Config (Box::into_raw).
    unsafe { rustty_config::c_api::rustty_config_finalize(cfg) };
    cfg
}
// opacity 0.5 (< 1.0) so the blur early-return is NOT taken; blur=.true (=> radius 20).
fn make_config_transparent_blur() -> *mut rustty_config::Config {
    let cfg = rustty_config_new_and_finalize();
    // SAFETY: `cfg` is a live Config; the raw fields are pub and read directly by
    // the blur leaf (`background_opacity`) + `background_blur_cval` (`background_blur`).
    unsafe {
        (*cfg).background_opacity = 0.5;
        (*cfg).background_blur = BackgroundBlur::True;
    }
    cfg
}
fn make_config_opaque() -> *mut rustty_config::Config {
    let cfg = rustty_config_new_and_finalize();
    // SAFETY: `cfg` is a live Config; opacity 1.0 selects the blur early-return path.
    unsafe { (*cfg).background_opacity = 1.0 };
    cfg
}

// The host `view` is a `MockHostView` double (a `Retained<MockHostView>`); its
// pointer crosses the C-ABI as `platform.macos.nsview`, and it doubles as the
// `window` arg for the blur leaf (it answers `windowNumber`).
struct HostedSurface {
    app: *mut App,
    surface: *mut Surface,
    view: Retained<MockHostView>,
}
impl HostedSurface {
    fn spawn() -> Option<Self> {
        Self::spawn_with(make_config_transparent_blur())
    }
    fn spawn_with(config: *mut rustty_config::Config) -> Option<Self> {
        ensure_init();
        let view = make_host_view();
        let nsview_ptr: *mut c_void = Retained::as_ptr(&view) as *mut c_void;
        // SAFETY: `test_options`/config are live for the call; app_new clones the config.
        let app = unsafe { app_new_serialized(&test_options(), config) };
        if app.is_null() {
            return None;
        }
        // SAFETY: default surface options built by the live C-ABI export.
        let mut opts: SurfaceOptions = unsafe { rustty_surface_config_new() };
        opts.platform_tag = 1; // PlatformTag::MacOS
        opts.platform = PlatformC {
            macos: PlatformCMacOS { nsview: nsview_ptr },
        };
        opts.scale_factor = 2.0; // Retina content-scale handshake
                                 // SAFETY: `app` is live; `opts` is a fully-initialized SurfaceOptions.
        let surface = unsafe { rustty_surface_new(app, &opts) };
        if surface.is_null() {
            // no Metal device => skip. Reclaim the app so the skip leaks nothing.
            // SAFETY: `app` is the live pointer from `rustty_app_new`.
            unsafe { rustty_app_free(app) };
            return None;
        }
        Some(HostedSurface { app, surface, view })
    }
    fn view_ptr(&self) -> *mut c_void {
        Retained::as_ptr(&self.view) as *mut c_void
    }
    // AppKit-layout stand-in: a real layer-hosting NSView lays its hosted layer out
    // to the view's bounds (in points); the renderer then reads `surfaceSize()` =
    // `layer.bounds * contentsScale` (metal_impl `surface_size`) to size the frame.
    // The inert `MockHostView` never lays anything out, so the attached
    // `IOSurfaceLayer` keeps its default 0×0 bounds and `drawFrame` early-returns at
    // the zero-dimension guard, never presenting. Mirror AppKit here: size the layer
    // to the host view's logical bounds (contentsScale = 2.0 was set at attach, so
    // 128×64 pt ⇒ a 256×128 px surface). Without this the renderer draws nothing.
    fn size_host_layer(&self) {
        use objc2_foundation::{NSPoint, NSRect, NSSize};
        // SAFETY: `self.view` is a live MockHostView answering `layer`.
        let l: *mut AnyObject = unsafe { msg_send![&*self.view, layer] };
        if l.is_null() {
            return;
        }
        let bounds = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(128.0, 64.0));
        // SAFETY: `l` is the live CALayer; `setBounds:` takes a CGRect (== NSRect).
        unsafe {
            let _: () = msg_send![l, setBounds: bounds];
        }
    }
    // CA-commit stand-in: in the live app the WindowServer/Core-Animation commit (and
    // the CVDisplayLink) fire the layer's `display` method, which invokes the display
    // callback the renderer installed at `loop_enter` → `drawFrame(sync=true)` →
    // `present(sync=true)` → `set_surface_sync` → `CALayer.setContents(surface)` — the
    // ON-MAIN-THREAD present path that needs NO dispatch-queue drain. The headless
    // harness runs no CA runloop, so we drive `display` ourselves each pump tick (the
    // renderer `golden_presents_frame_on_screen` precedent: `msg_send![layer,
    // display]`). This is the faithful headless equivalent of the CA commit that
    // presents on-screen in the real app; the render thread's own async
    // `present(sync=false)` hop (to the main dispatch queue) cannot complete headless
    // because no main-thread runloop drains that queue.
    fn drive_display(&self) {
        // SAFETY: `self.view` is a live MockHostView answering `layer`.
        let l: *mut AnyObject = unsafe { msg_send![&*self.view, layer] };
        if l.is_null() {
            return;
        }
        // SAFETY: `l` is the live IOSurfaceLayer; `-display` invokes its overridden
        // display method (fires the installed display callback under `draw_mutex`).
        unsafe {
            let _: () = msg_send![l, display];
        }
    }
    fn pump_bounded(&self) {
        // Give the render thread its host-view size (AppKit-layout stand-in) so
        // `surfaceSize()` is non-zero and frames actually draw.
        self.size_host_layer();
        let deadline = Instant::now() + Duration::from_secs(5);
        for _ in 0..2000 {
            // SAFETY: `self.app` is a live App pointer for the surface's lifetime.
            unsafe { rustty_app_tick(self.app) };
            // Drive the CA `display` present path (CA-commit stand-in).
            self.drive_display();
            if self.layer_has_contents() {
                break;
            }
            if Instant::now() > deadline {
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    // `[view layer]` -> the IOSurfaceLayer (a CALayer subclass) Metal::init attached.
    fn layer(&self) -> Option<Retained<AnyObject>> {
        // SAFETY: `self.view` is a live MockHostView; `layer` returns the stored id.
        let l: *mut AnyObject = unsafe { msg_send![&*self.view, layer] };
        // SAFETY: `l` is null (no handshake yet) or a live CALayer id — retain to hold it.
        unsafe { Retained::retain(l) }
    }
    // CALayer.contents() != nil (an IOSurface was presented).
    fn layer_has_contents(&self) -> bool {
        // SAFETY: `self.view` is a live MockHostView.
        let l: *mut AnyObject = unsafe { msg_send![&*self.view, layer] };
        if l.is_null() {
            return false;
        }
        // SAFETY: `l` is the live layer id; `contents` returns nil or the IOSurface id.
        let contents: *mut AnyObject = unsafe { msg_send![l, contents] };
        !contents.is_null()
    }
    // Lock the layer's IOSurface contents, read w*h*4 BGRA — mirrors Target::read_bgra
    // (IOSurfaceLockOptions::ReadOnly, base_address, bytes_per_row honored).
    fn read_presented_bgra(&self) -> Option<Vec<u8>> {
        use objc2_io_surface::{IOSurfaceLockOptions, IOSurfaceRef};
        // SAFETY: `self.view` is a live MockHostView.
        let l: *mut AnyObject = unsafe { msg_send![&*self.view, layer] };
        if l.is_null() {
            return None;
        }
        // SAFETY: `l` is the live layer id.
        let contents: *mut AnyObject = unsafe { msg_send![l, contents] };
        if contents.is_null() {
            return None;
        }
        // The layer's `contents` IS the presented IOSurfaceRef (the renderer present
        // path sets it via `CALayer.setContents(surface)`); reinterpret it as such.
        // SAFETY: `contents` is the live IOSurface the presenter just attached.
        let surface: &IOSurfaceRef = unsafe { &*(contents as *const IOSurfaceRef) };
        let w = surface.width(); // IOSurfaceGetWidth
        let h = surface.height(); // IOSurfaceGetHeight
        let bpr = surface.bytes_per_row(); // IOSurfaceGetBytesPerRow
        let w4 = w * 4;
        let mut out = vec![0u8; w * h * 4];
        // SAFETY: ReadOnly lock/unlock with a null seed; `base_address` is valid
        // between lock/unlock; we read `w4` bytes per row for `h` rows, all within the
        // surface allocation (`bpr >= w4`). Mirrors `Target::read_bgra`.
        unsafe {
            let _ = surface.lock(IOSurfaceLockOptions::ReadOnly, core::ptr::null_mut());
            let base = surface.base_address().as_ptr() as *const u8; // IOSurfaceGetBaseAddress
            for row in 0..h {
                core::ptr::copy_nonoverlapping(
                    base.add(row * bpr),
                    out.as_mut_ptr().add(row * w4),
                    w4,
                );
            }
            let _ = surface.unlock(IOSurfaceLockOptions::ReadOnly, core::ptr::null_mut());
        }
        Some(out)
    }

    // The presented IOSurface's pixel dimensions (None until a frame presented).
    fn presented_size(&self) -> Option<(usize, usize)> {
        use objc2_io_surface::IOSurfaceRef;
        // SAFETY: `self.view` is a live MockHostView answering `layer`.
        let l: *mut AnyObject = unsafe { msg_send![&*self.view, layer] };
        if l.is_null() {
            return None;
        }
        // SAFETY: `l` is the live layer id; `contents` returns nil or the IOSurface id.
        let contents: *mut AnyObject = unsafe { msg_send![l, contents] };
        if contents.is_null() {
            return None;
        }
        // SAFETY: `contents` IS the presented IOSurfaceRef (see read_presented_bgra).
        let surface: &IOSurfaceRef = unsafe { &*(contents as *const IOSurfaceRef) };
        Some((surface.width(), surface.height()))
    }

    // AppKit-layout stand-in for a RESIZE: re-lay the hosted layer to the new view
    // bounds (points). A real layer-hosting NSView does this on live resize.
    fn set_host_layer_bounds(&self, w_pt: f64, h_pt: f64) {
        use objc2_foundation::{NSPoint, NSRect, NSSize};
        // SAFETY: `self.view` is a live MockHostView answering `layer`.
        let l: *mut AnyObject = unsafe { msg_send![&*self.view, layer] };
        if l.is_null() {
            return;
        }
        let bounds = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(w_pt, h_pt));
        // SAFETY: `l` is the live CALayer; `setBounds:` takes a CGRect (== NSRect).
        unsafe {
            let _: () = msg_send![l, setBounds: bounds];
        }
    }

    // Swift-host stand-in for a CROSS-DPI MONITOR MOVE: re-stamp the hosted layer's
    // contentsScale exactly as `viewDidChangeBackingProperties` does
    // (`layer?.contentsScale = window.backingScaleFactor`,
    // macos/Sources/RusttyKit/Surface View/SurfaceView_AppKit.swift:857).
    fn set_host_layer_contents_scale(&self, scale: f64) {
        // SAFETY: `self.view` is a live MockHostView answering `layer`.
        let l: *mut AnyObject = unsafe { msg_send![&*self.view, layer] };
        if l.is_null() {
            return;
        }
        // SAFETY: `l` is the live CALayer; `setContentsScale:` takes a CGFloat.
        unsafe {
            let _: () = msg_send![l, setContentsScale: scale];
        }
    }

    // Bounded pump until the presented IOSurface reaches `expected` px dimensions.
    // Unlike `pump_bounded`, does NOT touch the layer geometry (the caller owns it —
    // `pump_bounded` would reset bounds to 128×64 and defeat resize/DPI tests).
    fn pump_until_presented_size(&self, expected: (usize, usize)) -> Option<(usize, usize)> {
        let deadline = Instant::now() + Duration::from_secs(5);
        for _ in 0..2000 {
            // SAFETY: `self.app` is a live App pointer for the surface's lifetime.
            unsafe { rustty_app_tick(self.app) };
            self.drive_display();
            let sz = self.presented_size();
            if sz == Some(expected) {
                return sz;
            }
            if Instant::now() > deadline {
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        self.presented_size()
    }
}
impl Drop for HostedSurface {
    fn drop(&mut self) {
        // SAFETY: `surface`/`app` are the live pointers this struct owns; freed once.
        unsafe {
            rustty_surface_free(self.surface);
            rustty_app_free(self.app);
        }
    }
}

// Task R13-C (UD62 follow-up): pins `scrollCallback`'s call into its lifted y
// and x helpers through the REAL path — the `rustty_surface_mouse_scroll` C
// entry point on a live surface. Precision offsets smaller than one cell only
// accumulate into `pending_scroll_x`/`pending_scroll_y`, so the two values say
// which offset reached which helper, with no scroll side effects. The
// reference's x block reads no multiplier and the y block multiplies by
// `mouse-scroll-multiplier`'s precision value (1.0 by default).
#[test]
fn mouse_scroll_routes_each_offset_to_its_own_axis() {
    let Some(host) = HostedSurface::spawn() else {
        eprintln!("no Metal device / app_new failed; skipping");
        return;
    };
    // SAFETY: `host.surface` is the live surface from `rustty_surface_new`; the
    // mouse state and the derived config are main-thread state, and this thread
    // is the one calling `scrollCallback`.
    let core = unsafe { &(*host.surface).core_surface };
    let cell = core.size.cell;
    let precision = core.config.mouse_scroll_multiplier.precision;
    let (x0, y0) = (core.mouse.pending_scroll_x, core.mouse.pending_scroll_y);
    assert_eq!((0.0, 0.0), (x0, y0), "a fresh surface has nothing pending");
    let (xoff, yoff) = (3.0, 7.0);
    assert!(
        xoff < f64::from(cell.width) && yoff * precision < f64::from(cell.height),
        "the offsets must stay below one cell ({cell:?}) so nothing scrolls"
    );

    // bit 0 of the mods is `precision`.
    // SAFETY: `host.surface` is live.
    unsafe { rustty_surface_mouse_scroll(host.surface, xoff, yoff, 1) };

    // SAFETY: as above; re-read after the call.
    let core = unsafe { &(*host.surface).core_surface };
    assert_eq!(
        (xoff, yoff * precision),
        (core.mouse.pending_scroll_x, core.mouse.pending_scroll_y),
        "xoff reaches the x helper and yoff the y helper"
    );
}

// UD68 probe (task R13-C2). A `#[global_allocator]` that forwards every call
// to `System` and, on `dealloc`, notes whether the freed pointer is the one
// this THREAD asked it to watch. Thread-local, so frees on parallel test
// threads cannot set it; and the watched string is live until freed, so its
// address cannot be reused by another allocation to fake the free.
struct DeallocWatch;

thread_local! {
    static WATCHED: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
    static WATCHED_FREED: core::cell::Cell<bool> = const { core::cell::Cell::new(false) };
}

unsafe impl std::alloc::GlobalAlloc for DeallocWatch {
    unsafe fn alloc(&self, layout: std::alloc::Layout) -> *mut u8 {
        unsafe { std::alloc::System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: std::alloc::Layout) {
        // `try_with`: never touch a thread-local that is being torn down.
        let _ = WATCHED.try_with(|w| {
            if w.get() != 0 && w.get() == ptr as usize {
                let _ = WATCHED_FREED.try_with(|f| f.set(true));
            }
        });
        unsafe { std::alloc::System.dealloc(ptr, layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: std::alloc::Layout) -> *mut u8 {
        unsafe { std::alloc::System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: std::alloc::Layout, new: usize) -> *mut u8 {
        unsafe { std::alloc::System.realloc(ptr, layout, new) }
    }
}

// D6-ALLOW(watching shim, not an allocator substitution): this is an
// integration-test binary (`tests/`), so it can never enter a shipped build.
// Every arm forwards to `std::alloc::System`, i.e. plain `malloc`/`free`, which
// the ASan/LSan interceptors still see; `dealloc` only compares a pointer
// against a thread-local first. Same shape as `rustty-apprt`'s
// `inherited_working_directory_tests` probe.
#[global_allocator]
static DEALLOC_WATCH: DeallocWatch = DeallocWatch;

// Task R13-C2 (UD68): the `clear()` call in `Surface::deinit` frees the last
// inherited working directory handed to the host, through the REAL path: a
// live surface whose terminal has a pwd, `rustty_surface_inherited_config`
// (→ `newSurfaceOptions` → `hand_out`), then `rustty_surface_free`
// (→ `App::closeSurface` → `Surface::deinit`). `closeSurface` frees the box
// as `ManuallyDrop<Surface>`, so without that `clear()` the string would leak,
// as it does in the reference.
#[test]
fn surface_free_frees_the_last_inherited_working_directory() {
    let Some(host) = HostedSurface::spawn() else {
        eprintln!("no Metal device / app_new failed; skipping");
        return;
    };
    let host = core::mem::ManuallyDrop::new(host);

    // Give the terminal a pwd, as an OSC 7 from the shell would, under the
    // renderer-state mutex the IO thread also takes.
    // SAFETY: `host.surface` is live; `renderer_state.mutex`/`terminal` are the
    // surface's own raw pointers, and the terminal is touched only under the
    // mutex.
    unsafe {
        let rs = &raw const (*host.surface).core_surface.renderer_state;
        (*(*rs).mutex).lock();
        (*(*rs).terminal).set_pwd(b"/tmp/r13c2-ud68").unwrap();
        (*(*rs).mutex).unlock();
    }

    // SAFETY: `host.surface` is live.
    let opts = unsafe {
        rustty_apprt_c::rustty_surface_inherited_config(
            host.surface,
            rustty_apprt_api::NewSurfaceContext::Tab,
        )
    };
    let wd = opts.working_directory;
    assert!(!wd.is_null(), "a tab inherits the terminal's pwd");
    // SAFETY: `wd` is owned by the surface until the next call or its free.
    assert_eq!(
        b"/tmp/r13c2-ud68",
        unsafe { std::ffi::CStr::from_ptr(wd) }.to_bytes()
    );

    WATCHED_FREED.with(|f| f.set(false));
    WATCHED.with(|w| w.set(wd as usize));
    // SAFETY: `host.surface` is the live surface; freed once, here.
    unsafe { rustty_surface_free(host.surface) };
    let freed = WATCHED_FREED.with(|f| f.get());
    WATCHED.with(|w| w.set(0));
    assert!(
        freed,
        "Surface::deinit frees the last inherited working directory"
    );

    // The rest of `HostedSurface::drop`, without the surface already freed.
    // SAFETY: `host.app` is live; `host.view` is read out once and dropped.
    unsafe {
        rustty_app_free(host.app);
        drop(core::ptr::read(&host.view));
    }
}

// Task R13-C2 (UD76): `imePoint`'s cursor sums with a saturated padding
// (UD71), the state `contentScaleCallback` leaves until the IO thread's
// resize shrinks the grid. The reference's unchecked `u32` sums overflow for
// any cursor off the first column or row; the port saturates. Driven through
// the real `rustty_surface_ime_point` on a live surface. The child is a silent
// `sleep`, so no output moves the cursor the test places.
#[test]
fn ime_point_saturates_with_a_saturated_padding() {
    let cfg = make_config_transparent_blur();
    // SAFETY: `cfg` is a live Config; `command` is a plain pub field.
    unsafe {
        (*cfg).command = Some(rustty_config::Command::Direct(vec![
            b"/bin/sleep".to_vec(),
            b"600".to_vec(),
        ]));
    }
    let Some(host) = HostedSurface::spawn_with(cfg) else {
        eprintln!("no Metal device / app_new failed; skipping");
        return;
    };
    // SAFETY: `host.surface` is live. The cursor is written under the
    // renderer-state mutex the IO thread and `imePoint` take; `size` is
    // main-thread state and this thread is the one calling `imePoint`.
    unsafe {
        let rs = &raw const (*host.surface).core_surface.renderer_state;
        (*(*rs).mutex).lock();
        (*(*rs).terminal).set_cursor_pos(3, 4); // 1-based: row 3, column 4
        (*(*rs).mutex).unlock();
        let size = &raw mut (*host.surface).core_surface.size;
        (*size).padding.left = u32::MAX;
        (*size).padding.top = u32::MAX;
    }
    let (mut x, mut y, mut w, mut h) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
    // SAFETY: `host.surface` is live; the four out-pointers are locals.
    unsafe { rustty_surface_ime_point(host.surface, &mut x, &mut y, &mut w, &mut h) };
    // Scale 2.0 (the harness's content scale): the saturated edge plus half a
    // cell (x) or a cell (y), halved.
    assert!(
        x >= f64::from(u32::MAX) / 2.0,
        "x from the saturated sum: {x}"
    );
    assert!(
        y >= f64::from(u32::MAX) / 2.0,
        "y from the saturated sum: {y}"
    );
}

#[test]
fn hosts_view_layer_and_presents_live_frame() {
    // UN-IGNORED (P0.1, 2026-07-12): W-ATTACH (commit 96b0116) wired the windowed
    // host-attach — `rustty_surface_new` now sends `setLayer:` to the host view via
    // `Renderer::finalizeSurfaceInit` → `Metal::attach_view`, so phase (1) passes.
    // NO MainThreadMarker guard — MockHostView is main-thread-agnostic.
    //
    // The remaining gap that kept it red was purely HEADLESS-HARNESS, not product:
    // (a) a real layer-hosting NSView lays its hosted layer out to the view bounds,
    //     which the renderer reads as `surfaceSize()` = `layer.bounds *
    //     contentsScale` (Metal.zig:217-218) — the inert MockHostView never lays out,
    //     so the attached IOSurfaceLayer kept its default 0×0 bounds and `drawFrame`
    //     early-returned at the zero-dimension guard, never presenting; and
    // (b) the render thread's `present(sync=false)` async-hops `set_surface` onto the
    //     MAIN dispatch queue, which no headless runloop drains.
    // `pump_bounded` now faithfully stands in for AppKit + the CA commit: it sizes
    // the hosted layer (a) and drives the layer's `-display` (b) — the reference
    // on-screen SYNC present path (`display` → installed displayCallback →
    // `drawFrame(true)` → `present(true)` → `setSurfaceSync`, Metal.zig:164-176 /
    // 253-259), the renderer `golden_presents_frame_on_screen` precedent. The Metal
    // probe is the skip guard for a no-Metal host.
    let host = match HostedSurface::spawn() {
        Some(h) => h,
        None => {
            eprintln!("no Metal device / app_new failed; skipping");
            return;
        }
    };
    // (1) HOST HANDSHAKE: the renderer attached an IOSurfaceLayer to our host view.
    assert!(host.layer().is_some(), "host view is not layer-hosting");
    // (2) FIRST ON-SCREEN PIXELS: pump until the render thread presents an IOSurface.
    host.pump_bounded();
    assert!(
        host.layer_has_contents(),
        "no IOSurface presented (no on-screen frame)"
    );
    // (3) GOLDEN GATE REUSE (STRUCTURAL, not exact-PNG): non-vacuous, view-sized.
    if let Some(px) = host.read_presented_bgra() {
        assert!(px.iter().any(|&b| b != 0), "presented frame is all-zero");
    }
    // (4) display_id: CGMainDisplayID pushed to the live render-thread mailbox.
    let display_id: u32 = objc2_core_graphics::CGMainDisplayID();
    // SAFETY: `host.surface` is the live surface owning a running render thread.
    unsafe { rustty_surface_set_display_id(host.surface, display_id) };
    host.pump_bounded();
    // (5) blur: transparent config => the real objc windowNumber + private CGS run.
    //     The MockHostView double answers `windowNumber`, so the CGS SPI is invoked
    //     for real (proving the #[link] CoreGraphics resolution end-to-end).
    // SAFETY: `host.app` is live; `view_ptr()` answers `windowNumber`.
    unsafe { rustty_set_window_background_blur(host.app, host.view_ptr()) };
    // Surviving all five phases IS the culmination.
}

// --- W4-2 focused sub-KAT ---
#[test]
fn set_window_background_blur_is_panic_free_on_transparent_config() {
    ensure_init();
    // SAFETY: `test_options`/config live for the call; app_new clones the config.
    let app = unsafe { app_new_serialized(&test_options(), make_config_transparent_blur()) };
    assert!(!app.is_null(), "app_new");
    let view = make_host_view();
    let wptr: *mut c_void = Retained::as_ptr(&view) as *mut c_void;
    // SAFETY: `app` is live; `wptr` answers `windowNumber`. opacity 0.5 => runs for real.
    unsafe { rustty_set_window_background_blur(app, wptr) };
    // DRIFT FIX (§6): the reference `ghostty_config_replace` does not exist here;
    // the real export is `rustty_app_update_config`.
    // SAFETY: `app` is live; the opaque config lives for the update call.
    unsafe { rustty_app_update_config(app, make_config_opaque()) };
    // SAFETY: `app` is live; opacity 1.0 => the blur early-return path.
    unsafe { rustty_set_window_background_blur(app, wptr) };
    // SAFETY: `app` is the live pointer from `rustty_app_new`.
    unsafe { rustty_app_free(app) };
    let _keep = view; // the host view outlives the calls
}

// --- W4-3 focused sub-KAT ---
#[test]
fn set_display_id_pushes_to_render_thread_panic_free() {
    // UN-IGNORED (M4, 2026-07-10): the old #[ignore] cited `Metal::loop_enter`
    // being `unimplemented!()`; that leaf landed in W5 and this KAT passes on a
    // Metal host (MIRI_EXCEPTIONS.md instructed the un-ignore). It spawns a LIVE
    // surface render thread and proves the display-id mailbox push is panic-free.
    let Some(host) = HostedSurface::spawn() else {
        return; // no Metal => skip
    };
    let display_id: u32 = objc2_core_graphics::CGMainDisplayID();
    // SAFETY: `host.surface` is the live surface owning a running render thread.
    unsafe { rustty_surface_set_display_id(host.surface, display_id) };
    host.pump_bounded(); // drain runs
    drop(host);
}

// --- WS2-A regression KAT ---
// The two Look-Up/accessibility exports must be ABORT-FREE on a LIVE surface:
// `quicklook_font` fires from force-click Look-Up AND every VoiceOver/accessibility
// query (SurfaceView_AppKit.swift:1521,1949,2361 in the oracle app), `quicklook_word`
// from the Look-Up gesture (:1512) — no explicit user intent required, so under the
// DG5 panic=abort policy any leftover `unimplemented!()` here is app-fatal.
// MACOS_APP_GAPS.md §3 listed both as open abort risks; the exports were in fact
// fully ported in P2.1/P2.2 (DEFERRALS.md P2 section). This KAT pins that they STAY
// real: it drives the actual C-ABI exports against a live surface (running render
// thread + populated CoreText font grid).
#[test]
fn quicklook_exports_are_abort_free_on_live_surface() {
    use objc2_core_foundation::{CFRetained, CFType};

    let Some(host) = HostedSurface::spawn() else {
        eprintln!("no Metal device / app_new failed; skipping");
        return;
    };
    host.pump_bounded(); // live render thread; the shared font grid is populated

    // quicklook_font: the live grid's primary CoreText face resolves ⇒ a +1
    // CTFontRef crosses the ABI (I16); the host balances it with exactly ONE
    // CFRelease — modeled as CFRetained::from_raw + drop (the P2.2 KAT's inverse).
    // SAFETY: `host.surface` is the live surface from `rustty_surface_new`.
    let font = unsafe { rustty_surface_quicklook_font(host.surface) };
    assert!(
        !font.is_null(),
        "primary CoreText face must resolve on a live surface"
    );
    // SAFETY: `font` is the +1 CTFontRef the export handed over; reclaiming it
    // consumes exactly that one reference (the host's CFRelease).
    drop(unsafe {
        CFRetained::<CFType>::from_raw(std::ptr::NonNull::new(font.cast::<CFType>()).unwrap())
    });

    // quicklook_word: MUST return without abort. `true` ⇒ `text` owns a heap
    // C-string freed via rustty_surface_free_text; `false` (no word under the
    // mouse-cursor cell — the harness never moved the mouse, so (0,0) is
    // typically empty) is a VALID no-word degrade. The assertion IS abort-freedom.
    let mut text = Text {
        tl_px_x: 0.0,
        tl_px_y: 0.0,
        offset_start: 0,
        offset_len: 0,
        text: core::ptr::null(),
        text_len: 0,
    };
    // SAFETY: `host.surface` is live; `text` is a writable Text for the call.
    let has_word = unsafe { rustty_surface_quicklook_word(host.surface, &mut text) };
    if has_word {
        assert!(!text.text.is_null(), "true return must populate the text");
        // SAFETY: `text` was populated by the export just above; freed exactly once.
        unsafe { rustty_surface_free_text(host.surface, &mut text) };
    }
}

// --- WS2-B cross-DPI KAT ---
// ORACLE OWNERSHIP (resolved, ghostty@4749c4e): the HOST owns the contentsScale
// re-stamp on a cross-DPI monitor move — `viewDidChangeBackingProperties` sets
// `layer.contentsScale = backingScaleFactor`, then calls
// `ghostty_surface_set_content_scale` + sends the new backing size
// (macos/Sources/Ghostty/Surface View/SurfaceView_AppKit.swift:836-872; re-driven
// on `windowDidChangeScreen`, :780-795). NO renderer mailbox content-scale message
// exists in the oracle (src/renderer/message.zig has none) — the renderer just
// READS `layer.contentsScale` in `surfaceSize()` (Metal.zig:217-218 ==
// metal_impl.rs `surface_size`). Rustty's Swift host is the faithful port
// (SurfaceView_AppKit.swift:837-871), so this KAT drives that exact host sequence
// through the C-ABI and asserts the presented frame follows: Retina (2.0) →
// non-Retina (1.0) at constant 128×64 pt must halve the pixel dimensions.
#[test]
fn content_scale_restamp_tracks_cross_dpi_move() {
    let Some(host) = HostedSurface::spawn() else {
        eprintln!("no Metal device / app_new failed; skipping");
        return;
    };
    host.pump_bounded();
    assert_eq!(
        host.presented_size(),
        Some((256, 128)),
        "first frame must be 128×64 pt @2x"
    );

    // The monitor move to a 1x display, exactly as the Swift host performs it:
    // (1) re-stamp the layer scale, (2) update the core content scale (font DPI),
    // (3) send the new backing pixel size.
    host.set_host_layer_contents_scale(1.0);
    // SAFETY: `host.surface` is live for both calls.
    unsafe { rustty_surface_set_content_scale(host.surface, 1.0, 1.0) };
    unsafe { rustty_surface_set_size(host.surface, 128, 64) };
    let sz = host.pump_until_presented_size((128, 64));
    assert_eq!(
        sz,
        Some((128, 64)),
        "presented IOSurface must track the re-stamped contentsScale (no blurry \
         stale-scale frame)"
    );
}

// --- WS2-D B4 KAT ---
// MACOS_APP_GAPS §2 B4: post-attach, `surfaceSize()` = layer.bounds ×
// contentsScale must FOLLOW a live resize. AppKit re-lays a layer-hosting view's
// layer on resize; the harness stands in (set_host_layer_bounds), then performs
// the Swift host's resize handshake (`setSurfaceSize` → rustty_surface_set_size
// with the BACKING pixel size, SurfaceView_AppKit.swift:869-871). The
// IOSurfaceLayer discards wrong-sized surfaces (IOSurfaceLayer.zig:104-116 ==
// iosurface_layer.rs), so convergence to the new size proves the whole reflow:
// core resize → renderer rebuild → present at bounds × scale.
#[test]
fn resize_tracks_host_layer_bounds() {
    let Some(host) = HostedSurface::spawn() else {
        eprintln!("no Metal device / app_new failed; skipping");
        return;
    };
    host.pump_bounded();
    assert!(host.layer_has_contents(), "no first frame presented");
    assert_eq!(
        host.presented_size(),
        Some((256, 128)),
        "first frame must be 128×64 pt @2x"
    );

    // Live resize to 200×100 pt (@2x ⇒ 400×200 px backing size).
    host.set_host_layer_bounds(200.0, 100.0);
    // SAFETY: `host.surface` is live.
    unsafe { rustty_surface_set_size(host.surface, 400, 200) };
    let sz = host.pump_until_presented_size((400, 200));
    assert_eq!(
        sz,
        Some((400, 200)),
        "presented IOSurface must track the layer resize (B4 reflow)"
    );
}

// --- on-device visible variant (#[ignore]): a REAL NSWindow, run ON THE MAIN THREAD ---
// This is the ONLY test that uses AppKit (`objc2-app-kit`, dev-only). It is #[ignore]
// and NOT part of the headless gate: it needs a live WindowServer/GUI session AND the
// process main thread. Under the default libtest harness it runs on a worker thread,
// where `MainThreadMarker::new()` is `None` and it early-returns with a note — to
// actually show the window, invoke it from a main-thread runner (e.g. a
// `harness = false` shim, or `cargo test -- --ignored --test-threads=1` from a GUI
// login session where the first frame lands on the main thread). It is a manual
// on-device confirmation, never a CI gate.
#[test]
#[ignore = "on-device: real NSWindow; needs a WindowServer + the process main thread"]
fn shows_visible_window_on_device() {
    use objc2::{MainThreadMarker, MainThreadOnly};
    use objc2_app_kit::{NSBackingStoreType, NSWindow, NSWindowStyleMask};
    use objc2_foundation::{NSPoint, NSRect, NSSize};

    let Some(mtm) = MainThreadMarker::new() else {
        eprintln!(
            "shows_visible_window_on_device: not on the AppKit main thread; \
                   run from a main-thread runner to see a real window. Skipping."
        );
        return;
    };
    ensure_init();
    // SAFETY: on the AppKit main thread (`mtm`); AppKit window construction.
    let window: Retained<NSWindow> = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(
            NSWindow::alloc(mtm),
            NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(200.0, 120.0)),
            NSWindowStyleMask::Titled,
            NSBackingStoreType::Buffered,
            false,
        )
    };
    let view = window.contentView().expect("contentView");
    let nsview_ptr: *mut c_void = Retained::as_ptr(&view) as *mut c_void;
    // SAFETY: `test_options`/config live for the call; app_new clones the config.
    let app = unsafe { app_new_serialized(&test_options(), make_config_transparent_blur()) };
    assert!(!app.is_null(), "app_new");
    // SAFETY: default surface options built by the live C-ABI export.
    let mut opts: SurfaceOptions = unsafe { rustty_surface_config_new() };
    opts.platform_tag = 1;
    opts.platform = PlatformC {
        macos: PlatformCMacOS { nsview: nsview_ptr },
    };
    opts.scale_factor = 2.0;
    // SAFETY: `app` is live; `opts` is a fully-initialized SurfaceOptions.
    let surface = unsafe { rustty_surface_new(app, &opts) };
    if surface.is_null() {
        // SAFETY: `app` is the live pointer from `rustty_app_new`.
        unsafe { rustty_app_free(app) };
        return; // no Metal => skip
    }
    // Makes the window VISIBLE (needs a WindowServer); safe on the main thread.
    window.makeKeyAndOrderFront(None);
    // bounded pump against the real view's layer contents.
    let deadline = Instant::now() + Duration::from_secs(5);
    for _ in 0..2000 {
        // SAFETY: `app` is live for the pump.
        unsafe { rustty_app_tick(app) };
        if Instant::now() > deadline {
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    // real windowNumber + private CGS blur on the live NSWindow.
    let wptr: *mut c_void = Retained::as_ptr(&window) as *mut c_void;
    // SAFETY: `app` is live; `wptr` is the live NSWindow id.
    unsafe { rustty_set_window_background_blur(app, wptr) };
    // SAFETY: `surface`/`app` are the live pointers, freed once here.
    unsafe {
        rustty_surface_free(surface);
        rustty_app_free(app)
    };
    let _keep = (window, view);
}

// Task R13-D2 (UD104, M35): `Surface::init`'s post-spawn error path must stop
// and join the renderer and io threads before it frees the state they use.
// The reference's errdefers free the thread loops and the terminal under the
// running threads; the port's `initErrdefers` joins first (task R13-D).
//
// Driven through the REAL `rustty_surface_new` on a live host (the harness
// above), with rustty-core's test-only `init-fault-inject` hook (a
// dev-dependency feature of this crate; see `rustty_core::init_fault`):
// - `IoSpawn` fails the io thread's spawn, with the renderer thread running;
// - `AfterIoSpawn` fails the next fallible step, where the `set_title` `try`
//   sits, with both threads running.
// The hook logs each spawn, stop, join and free on the init thread, and the
// test requires the exact sequence: every spawned thread stopped and joined,
// then the frees. At each free the probe also checks that no spawned thread
// is still unjoined; if one is (the old order), it prints why and `_exit`s
// BEFORE the free, so the failure is reported without touching freed memory.
// That is why each case runs in a child process: the parent asserts the
// child's exit status and output, and a violation fails the parent's
// assertion with the probe's reason instead of crashing the test runner.
#[test]
fn init_error_path_joins_both_threads_before_freeing_their_state() {
    use rustty_core::init_fault::{self, Event, Fault};
    const CHILD: &str = "RUSTTY_R13D2_INIT_FAULT_CHILD";
    const NAME: &str = "init_error_path_joins_both_threads_before_freeing_their_state";

    if let Some(which) = std::env::var_os(CHILD) {
        let fault = match which.to_str() {
            Some("io-spawn") => Fault::IoSpawn,
            Some("after-io-spawn") => Fault::AfterIoSpawn,
            other => panic!("unknown fault {other:?}"),
        };
        let quiet = || {
            let cfg = make_config_transparent_blur();
            // SAFETY: `cfg` is a live Config; `command` is a plain pub field.
            unsafe {
                (*cfg).command = Some(rustty_config::Command::Direct(vec![
                    b"/bin/sleep".to_vec(),
                    b"600".to_vec(),
                ]));
            }
            cfg
        };
        // Positive control: the same host with nothing armed creates a live
        // surface (no Metal device means there is nothing to test).
        let Some(host) = HostedSurface::spawn_with(quiet()) else {
            println!("R13D2-CHILD-SKIP no Metal device / app_new failed");
            return;
        };
        drop(host);

        ensure_init();
        let view = make_host_view();
        // SAFETY: `test_options`/config are live for the call.
        let app = unsafe { app_new_serialized(&test_options(), quiet()) };
        assert!(!app.is_null(), "app_new");
        // SAFETY: default surface options built by the live C-ABI export.
        let mut opts: SurfaceOptions = unsafe { rustty_surface_config_new() };
        opts.platform_tag = 1; // PlatformTag::MacOS
        opts.platform = PlatformC {
            macos: PlatformCMacOS {
                nsview: Retained::as_ptr(&view) as *mut c_void,
            },
        };
        opts.scale_factor = 2.0;

        init_fault::arm(fault);
        // SAFETY: `app` is live; `opts` is a fully-initialized SurfaceOptions.
        let surface = unsafe { rustty_surface_new(app, &opts) };
        let log = init_fault::take_log();
        assert!(
            surface.is_null(),
            "the injected fault fails rustty_surface_new"
        );

        let frees = [
            Event::Freed("io"),
            Event::Freed("io_thread"),
            Event::Freed("renderer_thread"),
            Event::Freed("renderer_state.mutex"),
        ];
        let mut expected = match fault {
            // The io thread never started; the renderer is stopped and joined.
            Fault::IoSpawn => vec![
                Event::RendererSpawned,
                Event::Injected(Fault::IoSpawn),
                Event::RendererStopped,
                Event::RendererJoined,
            ],
            // Both are stopped and joined, renderer first as in `deinit`.
            Fault::AfterIoSpawn => vec![
                Event::RendererSpawned,
                Event::IoSpawned,
                Event::Injected(Fault::AfterIoSpawn),
                Event::RendererStopped,
                Event::RendererJoined,
                Event::IoStopped,
                Event::IoJoined,
            ],
        };
        expected.extend(frees);
        assert_eq!(
            expected, log,
            "every spawned thread is stopped and joined before its state is freed"
        );

        // UD115: the failed init released its font-grid reference, so the app
        // holds no grid (the reference keeps it, and the app's teardown
        // `assert(font_grid_set.count() == 0)` then traps). Checked before
        // `rustty_app_free`, whose own check aborts across the C ABI.
        // SAFETY: `app` is live and no surface exists; `core_app` is its core.
        let grids = unsafe { (*(*app).core_app).font_grid_set.count() };
        assert_eq!(
            0, grids,
            "the failed Surface::init released its font-grid reference"
        );

        // SAFETY: `app` is the live pointer from `rustty_app_new`; freed once.
        unsafe { rustty_app_free(app) };
        drop(view);
        println!("R13D2-CHILD-OK {}", which.to_string_lossy());
        return;
    }

    let exe = std::env::current_exe().expect("current_exe");
    for which in ["io-spawn", "after-io-spawn"] {
        let out = std::process::Command::new(&exe)
            .args(["--exact", NAME, "--test-threads=1", "--nocapture"])
            .env(CHILD, which)
            .output()
            .expect("re-execute the test binary");
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        if stdout.contains("R13D2-CHILD-SKIP") {
            eprintln!("{which}: no Metal device / app_new failed; skipping");
            continue;
        }
        let violation = out.status.code() == Some(init_fault::VIOLATION_EXIT);
        assert!(
            !violation,
            "{which}: Surface::init's error path freed state under a running thread:\n{stderr}"
        );
        assert!(
            out.status.success() && stdout.contains(&format!("R13D2-CHILD-OK {which}")),
            "{which}: the child did not complete (status {:?}):\nstdout:\n{stdout}\nstderr:\n{stderr}",
            out.status
        );
    }
}
