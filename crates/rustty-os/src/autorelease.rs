//! Autorelease-pool shim.
//!
//! PORTS: the reference's vendored `objc` package `AutoreleasePool` as consumed by
//! `renderer/Thread.zig:338-346` (`drainMailbox`):
//!
//! ```zig
//! const pool = if (builtin.os.tag.isDarwin())
//!     @import("objc").AutoreleasePool.init()
//! else
//!     void;
//! defer if (builtin.os.tag.isDarwin()) pool.deinit();
//! ```
//!
//! On macOS this is a thin wrapper over `objc2::rc::autoreleasepool` — the SAME
//! Objective-C runtime pool (R3: a binding, not a reimplementation; the
//! `os/locale.rs:117` precedent). On every other target it is the identity, the
//! runtime analogue of the reference's comptime `void` arm — so the module is
//! unconditionally compiled (incl. wasm32) and callers need no cfg.

/// Run `f` inside an Objective-C autorelease pool (macOS); plain call elsewhere.
/// The closure-exit IS the reference `defer pool.deinit()`.
#[cfg(target_os = "macos")]
#[inline]
pub fn autoreleasepool<T, F: FnOnce() -> T>(f: F) -> T {
    objc2::rc::autoreleasepool(|_pool| f())
}

/// Non-Darwin arm: the reference comptime-selects `void` — identity.
#[cfg(not(target_os = "macos"))]
#[inline]
pub fn autoreleasepool<T, F: FnOnce() -> T>(f: F) -> T {
    f()
}

#[cfg(test)]
mod tests {
    // Cross-platform: the shim is value-transparent.
    #[test]
    fn returns_closure_value() {
        assert_eq!(super::autoreleasepool(|| 7), 7);
    }

    // macOS: the pool actually DRAINS — an object autoreleased inside the scope
    // is released when the closure exits (the memory-hygiene property the W3
    // deferral punted; a no-op shim would leave the +1 alive and fail this).
    #[cfg(target_os = "macos")]
    #[test]
    fn pool_drains_autoreleased_references() {
        use objc2::msg_send;
        use objc2::rc::Retained;
        use objc2::runtime::NSObject;

        fn retain_count(obj: &NSObject) -> usize {
            // SAFETY: `retainCount` is a plain NSUInteger-returning selector.
            unsafe { msg_send![obj, retainCount] }
        }

        let obj = NSObject::new();
        let baseline = retain_count(&obj);
        super::autoreleasepool(|| {
            // Clone (+1) and hand that reference to the ambient pool.
            let extra: Retained<NSObject> = obj.clone();
            let raw = Retained::into_raw(extra);
            // SAFETY: `raw` is a live +1 NSObject; `autorelease` transfers that
            // +1 to the innermost (our) pool and returns the object id.
            let _: *mut NSObject = unsafe { msg_send![raw, autorelease] };
            assert_eq!(retain_count(&obj), baseline + 1, "autoreleased +1 pending");
        });
        // Pool drained on scope exit: the pending +1 was released.
        assert_eq!(
            retain_count(&obj),
            baseline,
            "pool must drain on closure exit"
        );
    }
}
