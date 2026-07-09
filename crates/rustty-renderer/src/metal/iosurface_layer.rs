//! A wrapper around a CALayer with a utility method
//! for settings its `contents` to an IOSurface.
//!
//! PORTS: ghostty `src/renderer/metal/IOSurfaceLayer.zig` (R1: every line,
//! comment, and behavior). The public API is snake_case per the plan's Canonical
//! Interfaces (`init`/`release`/`set_surface`/`set_surface_sync`/
//! `set_display_callback`/`as_ca_layer`).
//!
//! PORT: `const log = std.log.scoped(.IOSurfaceLayer);` — routed to the log
//! facade where used; the wrong-size-discard message is preserved as a comment.

use std::cell::Cell;
use std::ffi::c_void;

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{define_class, msg_send, AnyThread, DefinedClass, MainThreadMarker};
use objc2_core_foundation::{CFRetained, CGFloat, CGRect, Type};
use objc2_io_surface::IOSurfaceRef;
use objc2_quartz_core::{kCAGravityTopLeft, CALayer};

// PORT: `var Subclass: ?objc.Class = null;` (lines 14-16):
//   We subclass CALayer with a custom display handler, we only need
//   to make the subclass once, and then we can use it as a singleton.
// In objc2 the `define_class!` macro registers the class pair EXACTLY ONCE, lazily
// at the first `IOSurfaceLayerClass::class()` (subsuming the `var Subclass` +
// `getSubclass()` singleton + `allocateClassPair`/`registerClassPair` dance).

/// PORT: the error set `error{ObjCFailed}` of `getSubclass()`/`init`. The objc2
/// class registration is infallible, so `init` always returns `Ok`; the type is
/// kept for the reference's `!void`/`error.ObjCFailed` parity.
#[derive(Debug)]
pub enum Error {
    ObjCFailed,
}

/// PORT: `pub const DisplayCallback = ?*align(8) const fn (?*anyopaque) void;`
/// (line 121). An optional C function pointer taking an opaque context.
pub type DisplayCallback = Option<unsafe extern "C-unwind" fn(*mut c_void)>;

// PORT: `subclass.addIvar("display_cb"); subclass.addIvar("display_ctx")` (lines
// 148-149). objc2 ivars can't hold an `Option<fn>`, so `DisplayCallback` is
// stored as raw bits (transmute round-trip) exactly as the reference stores it via
// `setInstanceVariable` of an objc id built from `@intFromPtr(display_cb)`. `Cell`
// gives interior mutability so `set_display_callback` can take `&self`.
pub struct Ivars {
    display_cb: Cell<*const c_void>,
    display_ctx: Cell<*mut c_void>,
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
        // PORT: `subclass.replaceMethod("display", ...)` (lines 151-162).
        #[unsafe(method(display))]
        fn display(&self) {
            let bits = self.ivars().display_cb.get();
            // PORT: `const display_cb: DisplayCallback = @ptrFromInt(@intFromPtr(
            //   self.getInstanceVariable("display_cb").value));`
            // DisplayCallback = Option<unsafe extern "C-unwind" fn(*mut c_void)>; a
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
            if let Some(cb) = cb {
                unsafe { cb(self.ivars().display_ctx.get()) };
            }
        }

        // PORT: Disable all animations for this layer by returning null for all
        // actions (`subclass.replaceMethod("actionForKey:", ...)`, lines 164-176).
        #[unsafe(method(actionForKey:))]
        fn action_for_key(&self, _key: *mut AnyObject) -> *mut AnyObject {
            // return objc.getClass("NSNull").?.msgSend(objc.c.id, "null", .{});
            unsafe { msg_send![objc2::class!(NSNull), null] }
        }
    }
);

// PORT: `const SetSurfaceBlock = objc.Block(struct { layer: objc.c.id,
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

/// PORT: `layer: objc.Object` — the underlying CALayer (our registered subclass).
pub struct IOSurfaceLayer {
    pub layer: Retained<IOSurfaceLayerClass>,
}

impl IOSurfaceLayer {
    pub fn init() -> Result<IOSurfaceLayer, Error> {
        // PORT (lines 21-30): The layer returned by `[CALayer layer]` is
        // autoreleased, which means that at the end of the current autorelease pool
        // it will be deallocated if it isn't retained, so we retain it here manually
        // an extra time. In objc2 `alloc().set_ivars(..).init()` nets the same
        // +1-owned subclass instance with no autorelease pool — behavior-equivalent.
        let this = IOSurfaceLayerClass::alloc().set_ivars(Ivars {
            // The ivars default to null, matching the reference's explicit
            // post-init `setInstanceVariable("display_cb"/"display_ctx", null)`
            // (lines 36-37).
            display_cb: Cell::new(core::ptr::null()),
            display_ctx: Cell::new(core::ptr::null_mut()),
        });
        let layer: Retained<IOSurfaceLayerClass> = unsafe { msg_send![super(this), init] };

        // PORT (lines 32-34): The layer gravity is set to top-left so that the
        // contents aren't stretched during resize operations before a new frame has
        // been drawn.
        layer.setContentsGravity(unsafe { kCAGravityTopLeft });

        Ok(IOSurfaceLayer { layer })
    }

    // PORT: `pub fn release(self: *IOSurfaceLayer) void { self.layer.release(); }`
    // (lines 42-44). Under ARC the `Retained<IOSurfaceLayerClass>` CFReleases when
    // `IOSurfaceLayer` drops; the Canonical Interface mandates `&mut self`, so this
    // documents the reference's manual release (the real teardown is the value drop).
    pub fn release(&mut self) {
        // no-op relative to Drop — the `Retained` releases on drop.
    }

    /// PORT: Canonical upcast to the base `CALayer` (for the W1 NSView attach).
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
        // PORT: `self.layer.setProperty("contents", surface);` (lines 83-85).
        let obj = (surface as *const IOSurfaceRef).cast::<AnyObject>();
        unsafe { self.layer.setContents(Some(&*obj)) };
    }

    // PORT: `fn setSurfaceCallback(block: *const SetSurfaceBlock.Context)` (lines
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
            return;
        }

        // layer.setProperty("contents", surface);
        let obj = (&*surface as *const IOSurfaceRef).cast::<AnyObject>();
        unsafe { layer.setContents(Some(&*obj)) };
    }

    pub fn set_display_callback(&self, display_cb: DisplayCallback, display_ctx: *mut c_void) {
        // PORT: `self.layer.setInstanceVariable("display_cb"/"display_ctx", ...)`
        // (lines 128-135). The fn pointer is stored as raw bits (null == None).
        self.layer
            .ivars()
            .display_cb
            .set(display_cb.map_or(core::ptr::null(), |f| f as *const c_void));
        self.layer.ivars().display_ctx.set(display_ctx);
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use std::ffi::c_void;
    use std::sync::atomic::{AtomicBool, Ordering};

    use objc2::msg_send;
    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2_io_surface::IOSurfaceRef;
    use rustty_render_types::AlphaBlending;

    use super::{DisplayCallback, IOSurfaceLayer};
    use crate::metal::metal_impl::{Error as MetalError, Metal};

    // W0.T3 culminating KAT: the CALayer subclass sets a real IOSurface as its
    // `contents`. Reuses the offscreen Metal device (the golden-gate device path)
    // to mint a real IOSurface-backed Target.
    #[test]
    fn iosurface_layer_sets_surface_as_contents() {
        let metal = match Metal::init_offscreen(AlphaBlending::Native, 64, 32) {
            Ok(m) => m,
            Err(MetalError::NoMetalDevice) => {
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
    unsafe extern "C-unwind" fn on_display(ctx: *mut c_void) {
        assert_eq!(ctx, 0xABCD_usize as *mut c_void);
        FIRED.store(true, Ordering::SeqCst);
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
