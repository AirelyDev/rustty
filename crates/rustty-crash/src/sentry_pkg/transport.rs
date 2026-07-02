//! rustty-crash::sentry_pkg::transport
//!
//! PORTS: ghostty pkg/sentry/transport.zig.
//!
//! PORT STATUS: done
#![allow(non_snake_case)]

// const std = @import("std");
// const assert = std.debug.assert;
use super::c;
use super::envelope::Envelope;

// PORT: the reference nests these in `Transport` (`Transport.SendFunc`);
// Rust inherent associated types are unstable, so they are hoisted to the
// module level.
// pub const SendFunc = *const fn (envelope: *Envelope, state: ?*anyopaque) callconv(.c) void;
pub type SendFunc = unsafe extern "C" fn(envelope: *mut Envelope, state: *mut std::ffi::c_void);
// pub const FreeFunc = *const fn (state: ?*anyopaque) callconv(.c) void;
pub type FreeFunc = unsafe extern "C" fn(state: *mut std::ffi::c_void);

/// sentry_transport_t
// PORT: `pub const Transport = opaque { ... }` — an opaque C type; only ever
// handled behind `*mut Transport` raw pointers.
#[repr(C)]
pub struct Transport {
    _opaque: [u8; 0],
}

impl Transport {
    // pub fn init(f: SendFunc) *Transport {
    pub fn init(f: SendFunc) -> *mut Transport {
        // return @ptrCast(c.sentry_transport_new(@ptrCast(f)).?);
        let ptr = unsafe {
            c::sentry_transport_new(Some(std::mem::transmute::<
                SendFunc,
                unsafe extern "C" fn(*mut c::sentry_envelope_t, *mut std::ffi::c_void),
            >(f)))
        };
        // `.?` — the reference safety-checks the optional.
        assert!(!ptr.is_null());
        ptr.cast::<Transport>()
    }

    // pub fn deinit(self: *Transport) void {
    /// # Safety
    ///
    /// `self_` must be a live transport pointer obtained from [`Transport::init`]
    /// whose ownership has not been passed to `sentry_options_set_transport`.
    pub unsafe fn deinit(self_: *mut Transport) {
        // c.sentry_transport_free(@ptrCast(self));
        unsafe { c::sentry_transport_free(self_.cast()) }
    }

    // pub fn setState(self: *Transport, state: ?*anyopaque) void {
    /// # Safety
    ///
    /// `self_` must be a live transport pointer obtained from [`Transport::init`].
    pub unsafe fn setState(self_: *mut Transport, state: *mut std::ffi::c_void) {
        unsafe { c::sentry_transport_set_state(self_.cast(), state) }
    }

    // pub fn setStateFreeFunc(self: *Transport, f: FreeFunc) void {
    /// # Safety
    ///
    /// `self_` must be a live transport pointer obtained from [`Transport::init`].
    pub unsafe fn setStateFreeFunc(self_: *mut Transport, f: FreeFunc) {
        unsafe { c::sentry_transport_set_free_func(self_.cast(), Some(f)) }
    }
}
