//! A wrapper around a CALayer with a utility method
//! for settings its `contents` to an IOSurface.
//!
//! PORTS: ghostty `src/renderer/metal/IOSurfaceLayer.zig` (R1: every line,
//! comment, and behavior). The public API is snake_case per the plan's Canonical
//! Interfaces (`init`/`release`/`set_surface`/`set_surface_sync`/
//! `set_display_callback`/`as_ca_layer`).
//!
//! PORT NOTE (task-14c-2): `const log = std.log.scoped(.IOSurfaceLayer);` — routed
//! to the `log` facade; the wrong-size-discard `log.debug` is RESTORED beneath its
//! R1 comment in `set_surface_callback`.

use std::ffi::c_void;
use std::sync::atomic::{AtomicPtr, Ordering};

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{define_class, msg_send, AnyThread, DefinedClass, MainThreadMarker};
use objc2_core_foundation::{CFRetained, CGFloat, CGRect, Type};
use objc2_io_surface::IOSurfaceRef;
use objc2_quartz_core::{kCAGravityTopLeft, CALayer};

/// `IOSurfaceLayer.zig:12`'s `const log = std.log.scoped(.IOSurfaceLayer);`.
///
/// The capitalisation is the REFERENCE's own scope name, carried verbatim (R1) —
/// every other scope in this crate is lower_snake because the reference's is.
const LOG_SCOPE: &str = "IOSurfaceLayer";

// PORT NOTE: `var Subclass: ?objc.Class = null;` (lines 14-16):
//   We subclass CALayer with a custom display handler, we only need
//   to make the subclass once, and then we can use it as a singleton.
// In objc2 the `define_class!` macro registers the class pair EXACTLY ONCE, lazily
// at the first `IOSurfaceLayerClass::class()` (subsuming the `var Subclass` +
// `getSubclass()` singleton + `allocateClassPair`/`registerClassPair` dance).

/// PORT NOTE: the error set `error{ObjCFailed}` of `getSubclass()`/`init`. The objc2
/// class registration is infallible, so `init` always returns `Ok`; the type is
/// kept for the reference's `!void`/`error.ObjCFailed` parity.
#[derive(Debug)]
pub enum Error {
    ObjCFailed,
}

/// PORT NOTE: `pub const DisplayCallback = ?*align(8) const fn (?*anyopaque) void;`
/// (line 121). An optional C function pointer taking an opaque context.
// PORT NOTE (§3c.3): plain `extern "C"`, never the unwinding C ABI variant — a
// Zig fn never unwinds (a Zig panic aborts), and this callback is invoked from
// the ObjC `display` override inside Core Animation frames, so an unwind across
// it would be UB in the caller. `extern "C"` makes any escaping panic abort at
// the boundary (the Zig-faithful failure mode, PORT_RULES DG5); the installed
// `display_callback` additionally catch_unwind+aborts explicitly (metal_impl.rs).
pub type DisplayCallback = Option<unsafe extern "C" fn(*mut c_void)>;

// PORT NOTE: `subclass.addIvar("display_cb"); subclass.addIvar("display_ctx")` (lines
// 148-149). objc2 ivars can't hold an `Option<fn>`, so `DisplayCallback` is
// stored as raw bits (transmute round-trip) exactly as the reference stores it via
// `setInstanceVariable` of an objc id built from `@intFromPtr(display_cb)`.
// `AtomicPtr` gives interior mutability so `set_display_callback` can take `&self`
// AND makes the cross-thread pairing formally defined: the reference's plain ObjC
// ivars are unsynchronized, and a non-atomic `Cell` written on one thread while
// `display` reads on another is a data race — UB (VALIDATION_REPORT §3c.10; race
// reproduced + fix proven on the isolated Miri harness,
// .port/phase-2/mirihar-chunk3b `ivars_naive` UB / `ivars_shipped` CLEAN).
//
// PORT NOTE: reference defect (R13), UD70. This note used to say that
// `setDisplayCallback` and the CA `display` both fire on the main thread. They do
// not: `setDisplayCallback` is reached from `loopEnter`, which the RENDER thread
// calls (`renderer/Thread.zig:227-228` → `generic.zig:902-906` →
// `Metal.zig:164-170`), while CoreAnimation calls `display` on the main thread
// (for example on a bounds change, `needsDisplayOnBoundsChange`). Atomics remove
// the data race but not the torn PAIR: the reference stores `display_cb` and then
// `display_ctx`, so a `display` between the two stores calls the callback with a
// null renderer. `set_display_callback` and `display` below close that window.
pub struct Ivars {
    display_cb: AtomicPtr<c_void>,
    display_ctx: AtomicPtr<c_void>,
}

define_class!(
    // SAFETY:
    // - Superclass CALayer has no subclassing requirements beyond ObjC.
    // - IOSurfaceLayerClass does not implement Drop.
    #[unsafe(super(CALayer))]
    #[name = "IOSurfaceLayer"]
    #[ivars = Ivars]
    pub struct IOSurfaceLayerClass;

    impl IOSurfaceLayerClass {
        // PORT NOTE: `subclass.replaceMethod("display", ...)` (lines 151-162).
        #[unsafe(method(display))]
        fn display(&self) {
            let bits: *mut c_void = self.ivars().display_cb.load(Ordering::SeqCst);
            // PORT NOTE: `const display_cb: DisplayCallback = @ptrFromInt(@intFromPtr(
            //   self.getInstanceVariable("display_cb").value));`
            // DisplayCallback = Option<unsafe extern "C" fn(*mut c_void)>; a
            // null `*const c_void` transmutes to `None` (fn-pointer niche), a
            // non-null to `Some(fn)` — the Rust analogue of the reference's
            // `@ptrFromInt(@intFromPtr(...))`.
            const {
                assert!(
                    core::mem::size_of::<*const c_void>() == core::mem::size_of::<DisplayCallback>()
                )
            };
            let cb: DisplayCallback = unsafe { core::mem::transmute(bits) };
            // if (display_cb) |cb| cb(@ptrCast(self.getInstanceVariable("display_ctx").value));
            //
            // PORT NOTE: reference defect (R13), UD70. The reference calls the
            // callback with whatever `display_ctx` holds, which is null if
            // `display` runs between `setDisplayCallback`'s two stores (render
            // thread vs main thread); `displayCallback` then dereferences a null
            // renderer. A null context is skipped here, the same as the
            // reference's own no-callback arm (no draw on this pass).
            if let Some(cb) = cb {
                let ctx = self.ivars().display_ctx.load(Ordering::SeqCst);
                if !ctx.is_null() {
                    unsafe { cb(ctx) };
                }
            }
        }

        // PORT NOTE: Disable all animations for this layer by returning null for all
        // actions (`subclass.replaceMethod("actionForKey:", ...)`, lines 164-176).
        #[unsafe(method(actionForKey:))]
        fn action_for_key(&self, _key: *mut AnyObject) -> *mut AnyObject {
            // return objc.getClass("NSNull").?.msgSend(objc.c.id, "null", .{});
            unsafe { msg_send![objc2::class!(NSNull), null] }
        }
    }
);

// PORT NOTE: `const SetSurfaceBlock = objc.Block(struct { layer: objc.c.id,
// surface: *IOSurface }, .{}, void);` (lines 87-90). The block context struct that
// carries the layer id + retained surface across the main-thread dispatch hop.
// `Send` is sound: `layer` is a retained CALayer id (thread-safe to
// setProperty:contents on the main queue) and IOSurfaceRef is Send/Sync.
struct SetSurfaceArg {
    // objc.c.id — retained +1 for the hop (see the NOTE in `set_surface`).
    layer: *mut IOSurfaceLayerClass,
    // retained; CFReleases when the closure drops (== ref `defer surface.release()`).
    surface: CFRetained<IOSurfaceRef>,
}

// SAFETY: see the struct comment — the layer is a retained id safe to touch on the
// main queue, and CFRetained<IOSurfaceRef> is Send.
unsafe impl Send for SetSurfaceArg {}

/// PORT NOTE: `layer: objc.Object` — the underlying CALayer (our registered subclass).
pub struct IOSurfaceLayer {
    pub layer: Retained<IOSurfaceLayerClass>,
}

impl IOSurfaceLayer {
    pub fn init() -> Result<IOSurfaceLayer, Error> {
        // The layer returned by `[CALayer layer]` is autoreleased, which means
        // that at the end of the current autorelease pool it will be deallocated
        // if it isn't retained, so we retain it here manually an extra time.
        //
        // PORT NOTE (lines 21-30): in objc2 `alloc().set_ivars(..).init()` nets the
        // same +1-owned subclass instance with no autorelease pool — behavior-equivalent.
        let this = IOSurfaceLayerClass::alloc().set_ivars(Ivars {
            // The ivars default to null, matching the reference's explicit
            // post-init `setInstanceVariable("display_cb"/"display_ctx", null)`
            // (lines 36-37).
            display_cb: AtomicPtr::new(core::ptr::null_mut()),
            display_ctx: AtomicPtr::new(core::ptr::null_mut()),
        });
        let layer: Retained<IOSurfaceLayerClass> = unsafe { msg_send![super(this), init] };

        // The layer gravity is set to top-left so that the contents aren't
        // stretched during resize operations before a new frame has been drawn.
        // (lines 32-34)
        layer.setContentsGravity(unsafe { kCAGravityTopLeft });

        Ok(IOSurfaceLayer { layer })
    }

    // PORT NOTE: `pub fn release(self: *IOSurfaceLayer) void { self.layer.release(); }`
    // (lines 42-44). Under ARC the `Retained<IOSurfaceLayerClass>` CFReleases when
    // `IOSurfaceLayer` drops; the Canonical Interface mandates `&mut self`, so this
    // documents the reference's manual release (the real teardown is the value drop).
    pub fn release(&mut self) {
        // The layer may be retained by the view after we release our reference.
        // Clear the callback first so that a later display pass can't access the
        // renderer that owned this wrapper after it has been freed.
        self.set_display_callback(None, core::ptr::null_mut());
        // The layer release itself is a no-op relative to Drop — the `Retained`
        // releases on drop.
    }

    /// PORT NOTE: Canonical upcast to the base `CALayer` (for the W1 NSView attach).
    /// `define_class super(CALayer)` ⇒ `Deref<Target = CALayer>`.
    pub fn as_ca_layer(&self) -> &CALayer {
        &self.layer
    }

    /// Sets the layer's `contents` to the provided IOSurface.
    ///
    /// Makes sure to do so on the main thread to avoid visual artifacts.
    pub fn set_surface(&self, surface: &IOSurfaceRef) -> Result<(), Error> {
        // We retain the surface to make sure it's not GC'd
        // before we can set it as the contents of the layer.
        //
        // We release in the callback after setting the contents.
        let owned: CFRetained<IOSurfaceRef> = surface.retain();
        // NOTE: Since `self.layer` is passed as an `objc.c.id`, it's
        //       automatically retained when the block is copied, so we
        //       don't need to retain it ourselves like with the surface.
        //       Here we retain it explicitly for the raw id we move into the arg.
        let layer_id: *mut IOSurfaceLayerClass = {
            let r: Retained<IOSurfaceLayerClass> = self.layer.clone(); // +1
            Retained::into_raw(r)
        };
        let arg = SetSurfaceArg {
            layer: layer_id,
            surface: owned,
        };

        // We check if we're on the main thread and run the block directly if so.
        if MainThreadMarker::new().is_some() {
            Self::set_surface_callback(arg);
        } else {
            // NOTE: The block will be copied when we pass it to dispatch_async,
            //       and then automatically be deallocated by the objc runtime
            //       once it's executed.
            dispatch2::DispatchQueue::main().exec_async(move || Self::set_surface_callback(arg));
        }
        Ok(())
    }

    /// Sets the layer's `contents` to the provided IOSurface.
    ///
    /// Does not ensure this happens on the main thread.
    pub fn set_surface_sync(&self, surface: &IOSurfaceRef) {
        // PORT NOTE: `self.layer.setProperty("contents", surface);` (lines 83-85).
        let obj = (surface as *const IOSurfaceRef).cast::<AnyObject>();
        unsafe { self.layer.setContents(Some(&*obj)) };
    }

    // PORT NOTE: `fn setSurfaceCallback(block: *const SetSurfaceBlock.Context)` (lines
    // 92-119). The main-thread body that actually sets `contents`.
    fn set_surface_callback(arg: SetSurfaceArg) {
        // const layer = objc.Object.fromId(block.layer);
        // Reconstruct the retained layer id; it drops (CFRelease) at fn end == the
        // block's auto-release.
        let layer: Retained<IOSurfaceLayerClass> =
            unsafe { Retained::from_raw(arg.layer).unwrap() };
        // const surface: *IOSurface = block.surface;
        //
        // See explanation of why we retain and release in `setSurface`.
        // defer surface.release();  -> `surface` drops (CFRelease) at fn end.
        let surface = arg.surface;

        // We check to see if the surface is the appropriate size for
        // the layer, if it's not then we discard it. This is because
        // asynchronously drawn frames can sometimes finish just after
        // a synchronously drawn frame during a resize, and if we don't
        // discard the improperly sized surface it creates jank.
        let bounds: CGRect = layer.bounds();
        let scale: CGFloat = layer.contentsScale();
        let width = (bounds.size.width * scale) as usize;
        let height = (bounds.size.height * scale) as usize;
        if width != surface.width() || height != surface.height() {
            // log.debug(
            //     "setSurfaceCallback(): surface is wrong size for layer, discarding. surface = {d}x{d}, layer = {d}x{d}",
            //     .{ surface.getWidth(), surface.getHeight(), width, height },
            // );
            log::debug!(
                target: LOG_SCOPE,
                "setSurfaceCallback(): surface is wrong size for layer, discarding. surface = {}x{}, layer = {width}x{height}",
                surface.width(),
                surface.height()
            );
            return;
        }

        // layer.setProperty("contents", surface);
        let obj = (&*surface as *const IOSurfaceRef).cast::<AnyObject>();
        unsafe { layer.setContents(Some(&*obj)) };
    }

    pub fn set_display_callback(&self, display_cb: DisplayCallback, display_ctx: *mut c_void) {
        // PORT NOTE: `self.layer.setInstanceVariable("display_cb"/"display_ctx", ...)`
        // (lines 128-135). The fn pointer is stored as raw bits (null == None);
        // atomic stores per the Ivars PORT NOTE (§3c.10).
        //
        // PORT NOTE: reference defect (R13), UD70. The reference always stores
        // `display_cb` first, so while INSTALLING a callback (render thread,
        // `loopEnter`) a main-thread `display` between the stores sees the new
        // callback with a still-null context. The port orders the stores so a
        // reader only ever sees "no callback" or "a callback with its context":
        // installing stores the context first; clearing (`release`/`Drop`) clears
        // the callback first, which is the reference's own clearing order.
        let ivars = self.layer.ivars();
        let cb_bits =
            display_cb.map_or(core::ptr::null_mut(), |f| f as *const c_void as *mut c_void);
        if cb_bits.is_null() {
            ivars.display_cb.store(cb_bits, Ordering::SeqCst);
            #[cfg(all(test, target_os = "macos"))]
            tests::between_display_stores(self);
            ivars.display_ctx.store(display_ctx, Ordering::SeqCst);
        } else {
            ivars.display_ctx.store(display_ctx, Ordering::SeqCst);
            #[cfg(all(test, target_os = "macos"))]
            tests::between_display_stores(self);
            ivars.display_cb.store(cb_bits, Ordering::SeqCst);
        }
    }
}

// PORT NOTE (4b4a5b241): in the reference the callback-clear lives in `release`;
// in the port the real teardown is this value Drop (see the `release` PORT NOTE),
// so the clear runs here too — every teardown path reaches it.
impl Drop for IOSurfaceLayer {
    fn drop(&mut self) {
        // The layer may be retained by the view after we release our reference.
        // Clear the callback first so that a later display pass can't access the
        // renderer that owned this wrapper after it has been freed.
        self.set_display_callback(None, core::ptr::null_mut());
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use std::ffi::c_void;
    use std::sync::atomic::{AtomicBool, Ordering};

    use objc2::msg_send;
    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2::DefinedClass;
    use objc2_io_surface::IOSurfaceRef;
    use rustty_render_types::AlphaBlending;

    use super::{DisplayCallback, IOSurfaceLayer};

    // W0.T3 culminating KAT: the CALayer subclass sets a real IOSurface as its
    // `contents`. Reuses the offscreen Metal device (the golden-gate device path)
    // to mint a real IOSurface-backed Target.
    #[test]
    fn iosurface_layer_sets_surface_as_contents() {
        let metal = match crate::metal::test_support::offscreen_metal(AlphaBlending::Native, 64, 32)
        {
            Ok(m) => m,
            // No Metal device (the test device's `NoMetalDevice`), or a failed
            // `newCommandQueue`.
            Err(e) => {
                // F5 strict-mode pivot (metal::test_support).
                crate::metal::test_support::require_metal_or_skip(
                    "iosurface_layer_sets_surface_as_contents",
                    &format!("{e:?}"),
                );
                eprintln!("no Metal device; skipping");
                return;
            }
        };
        let target = metal.init_target(64, 32).expect("init_target");

        let layer = IOSurfaceLayer::init().expect("IOSurfaceLayer::init");
        // Freshly-inited layer has no contents.
        let contents0: Option<Retained<AnyObject>> = unsafe { layer.layer.contents() };
        assert!(contents0.is_none());

        // set_surface_sync sets the IOSurface as `contents` (no main-thread hop).
        layer.set_surface_sync(&target.surface);
        let contents1: Retained<AnyObject> =
            unsafe { layer.layer.contents() }.expect("contents set");

        // Pointer identity: CALayer stores the same IOSurface object we handed it.
        let want = (&*target.surface as *const IOSurfaceRef).cast::<c_void>();
        let got = Retained::as_ptr(&contents1) as *const c_void;
        assert_eq!(got, want, "contents id must be the IOSurface we set");
    }

    static FIRED: AtomicBool = AtomicBool::new(false);
    unsafe extern "C" fn on_display(ctx: *mut c_void) {
        assert_eq!(ctx, 0xABCD_usize as *mut c_void);
        FIRED.store(true, Ordering::SeqCst);
    }

    static FIRED_AFTER_DROP: AtomicBool = AtomicBool::new(false);
    unsafe extern "C" fn on_display_after_drop(_ctx: *mut c_void) {
        FIRED_AFTER_DROP.store(true, Ordering::SeqCst);
    }

    // 4b4a5b241 — renderer/metal: clear callback before layer release. The host
    // view can retain the backing layer beyond renderer teardown; a later Core
    // Animation display pass must not invoke the callback with a freed renderer
    // context.
    #[test]
    fn iosurface_layer_drop_clears_display_callback() {
        FIRED_AFTER_DROP.store(false, Ordering::SeqCst);
        let layer = IOSurfaceLayer::init().expect("IOSurfaceLayer::init");
        let cb: DisplayCallback = Some(on_display_after_drop);
        layer.set_display_callback(cb, 0xDEAD_usize as *mut c_void);

        // The host view's retention of the backing CALayer, outliving the wrapper.
        let host_retained = layer.layer.clone();
        drop(layer); // renderer teardown

        // Drop must have cleared the stored callback + ctx ivars...
        assert!(
            host_retained
                .ivars()
                .display_cb
                .load(Ordering::SeqCst)
                .is_null(),
            "Drop must clear the display callback"
        );
        assert!(
            host_retained
                .ivars()
                .display_ctx
                .load(Ordering::SeqCst)
                .is_null(),
            "Drop must clear the display context"
        );
        // ...so a later display pass on the host-retained layer is a no-op.
        let _: () = unsafe { msg_send![&*host_retained, display] };
        assert!(
            !FIRED_AFTER_DROP.load(Ordering::SeqCst),
            "display after Drop must not invoke the cleared callback"
        );
    }

    // ---- UD70 (R13) ----------------------------------------------------------
    //
    // The race is between the render thread's `set_display_callback` and a
    // main-thread CoreAnimation `display`; a real CA schedule cannot be driven
    // deterministically, so `set_display_callback` calls
    // `between_display_stores` between its two ivar stores. It exists only in
    // this crate's own test build (`#[cfg(all(test, target_os = "macos"))]` at
    // the call site and here), and does nothing unless a test on the SAME
    // thread installed a hook. The hook plays the main thread: it inspects the
    // ivars and sends `-display` at exactly the moment the race would.

    type Hook = Box<dyn FnMut(&IOSurfaceLayer)>;

    thread_local! {
        static BETWEEN_STORES: std::cell::RefCell<Option<Hook>> =
            const { std::cell::RefCell::new(None) };
        static SEEN_CTX: std::cell::RefCell<Vec<usize>> =
            const { std::cell::RefCell::new(Vec::new()) };
    }

    pub(super) fn between_display_stores(layer: &IOSurfaceLayer) {
        let hook = BETWEEN_STORES.with(|h| h.borrow_mut().take());
        if let Some(mut hook) = hook {
            hook(layer);
            BETWEEN_STORES.with(|h| *h.borrow_mut() = Some(hook));
        }
    }

    fn set_hook(hook: Option<Hook>) {
        BETWEEN_STORES.with(|h| *h.borrow_mut() = hook);
    }

    unsafe extern "C" fn record_ctx(ctx: *mut c_void) {
        SEEN_CTX.with(|v| v.borrow_mut().push(ctx as usize));
    }

    fn seen() -> Vec<usize> {
        SEEN_CTX.with(|v| std::mem::take(&mut *v.borrow_mut()))
    }

    /// What the hook observed at the gap: `(cb is null, ctx as usize)`, and the
    /// contexts `-display` passed to the callback right then.
    type Gap = std::rc::Rc<std::cell::RefCell<Vec<(bool, usize, Vec<usize>)>>>;

    fn gap_probe() -> (Gap, Hook) {
        let gap: Gap = Default::default();
        let g = gap.clone();
        let hook: Hook = Box::new(move |layer: &IOSurfaceLayer| {
            let iv = layer.layer.ivars();
            let cb_null = iv.display_cb.load(Ordering::SeqCst).is_null();
            let ctx = iv.display_ctx.load(Ordering::SeqCst) as usize;
            let _: () = unsafe { msg_send![&*layer.layer, display] };
            g.borrow_mut().push((cb_null, ctx, seen()));
        });
        (gap, hook)
    }

    /// Installing a callback (the render thread's `loopEnter`): a `display`
    /// between the two stores must not see the callback without its context.
    /// The reference stores the callback first, so this `display` calls it with
    /// a null renderer.
    #[test]
    fn a_display_while_the_callback_is_installed_never_gets_a_null_context() {
        let _ = seen();
        let layer = IOSurfaceLayer::init().expect("IOSurfaceLayer::init");
        let (gap, hook) = gap_probe();
        set_hook(Some(hook));
        layer.set_display_callback(Some(record_ctx), 0xABCD_usize as *mut c_void);
        set_hook(None);

        let gap = gap.borrow().clone();
        assert_eq!(1, gap.len(), "the hook ran once, between the two stores");
        let (cb_null, ctx, called_with) = &gap[0];
        assert!(
            called_with.is_empty(),
            "display between the stores must not call the callback; it got {called_with:?}"
        );
        assert!(
            *cb_null,
            "the callback is published last, after its context"
        );
        assert_eq!(0xABCD, *ctx, "the context is stored first");

        // Once installed, `display` calls it with its context.
        let _: () = unsafe { msg_send![&*layer.layer, display] };
        assert_eq!(vec![0xABCD], seen());
    }

    /// Clearing (`release`/`Drop`) keeps the reference's own order: the
    /// callback goes first, so a `display` in between calls nothing.
    #[test]
    fn a_display_while_the_callback_is_cleared_calls_nothing() {
        let _ = seen();
        let layer = IOSurfaceLayer::init().expect("IOSurfaceLayer::init");
        layer.set_display_callback(Some(record_ctx), 0xABCD_usize as *mut c_void);
        let (gap, hook) = gap_probe();
        set_hook(Some(hook));
        layer.set_display_callback(None, core::ptr::null_mut());
        set_hook(None);

        let gap = gap.borrow().clone();
        assert_eq!(1, gap.len(), "the hook ran once, between the two stores");
        assert_eq!((true, 0xABCD, vec![]), gap[0]);
    }

    /// `display` itself skips a callback whose context is null, whatever the
    /// store order that produced the pair.
    #[test]
    fn display_skips_a_callback_whose_context_is_null() {
        let _ = seen();
        let layer = IOSurfaceLayer::init().expect("IOSurfaceLayer::init");
        let iv = layer.layer.ivars();
        let cb: DisplayCallback = Some(record_ctx);
        iv.display_cb.store(
            cb.map_or(core::ptr::null_mut(), |f| f as *const c_void as *mut c_void),
            Ordering::SeqCst,
        );
        let _: () = unsafe { msg_send![&*layer.layer, display] };
        assert!(seen().is_empty(), "no call with a null context");

        // The same callback runs once its context is there.
        iv.display_ctx
            .store(0x1234_usize as *mut c_void, Ordering::SeqCst);
        let _: () = unsafe { msg_send![&*layer.layer, display] };
        assert_eq!(vec![0x1234], seen());
    }

    // The overridden `display` method invokes the stored display callback+ctx.
    #[test]
    fn iosurface_layer_display_callback_fires() {
        FIRED.store(false, Ordering::SeqCst);
        let layer = IOSurfaceLayer::init().expect("IOSurfaceLayer::init");
        let cb: DisplayCallback = Some(on_display);
        layer.set_display_callback(cb, 0xABCD_usize as *mut c_void);
        // Send -display directly (the CVDisplayLink/present path invokes it live).
        let _: () = unsafe { msg_send![&*layer.layer, display] };
        assert!(FIRED.load(Ordering::SeqCst), "display callback must fire");
    }
}
