//! common.rs — faithful port of libxev `watcher/common.zig` (D6, XEV_BLUEPRINT
//! §2.2 chunk C3).
//!
//! The typed→untyped userdata bridge every watcher trampoline (`Async.wait`,
//! `Timer.run`, …) uses. In C3 only the loop core lands, so this is sequenced
//! ahead of its first consumer (the C4+ watcher trampolines) and is
//! `allow(dead_code)` until then.

use core::any::TypeId;
use core::ffi::c_void;

/// Convert the callback value with an opaque pointer into the userdata type
/// that we can pass to our higher level callback types.
//
// PORT NOTE: the reference `pub fn userdataValue(comptime Userdata: type, v:
// ?*anyopaque) ?*Userdata` special-cases the Zig `void` type ("Void userdata is
// always a null pointer"). Rust's stand-in for "no userdata" is the zero-sized
// unit `()`, so the comptime `Userdata == void` check maps to a runtime
// `TypeId::of::<Userdata>() == TypeId::of::<()>()` (requiring `Userdata:
// 'static`, which every watcher userdata is). The non-void path is the faithful
// `@ptrCast(@alignCast(v))` — a raw pointer reinterpret.
#[allow(dead_code)]
pub fn userdata_value<Userdata: 'static>(v: *mut c_void) -> *mut Userdata {
    // Void userdata is always a null pointer.
    if TypeId::of::<Userdata>() == TypeId::of::<()>() {
        return core::ptr::null_mut();
    }
    v as *mut Userdata
}
