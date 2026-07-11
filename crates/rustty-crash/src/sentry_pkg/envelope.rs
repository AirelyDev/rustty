//! rustty-crash::sentry_pkg::envelope
//!
//! PORTS: ghostty pkg/sentry/envelope.zig.
//!
//! PORT STATUS: done
#![allow(non_snake_case)]

// const std = @import("std");
// const assert = std.debug.assert;
use super::c;
use super::value::Value;

/// PORT NOTE: `error.WriteFailed` (the inferred error set of `writeToFile`).
#[derive(Debug, PartialEq, Eq)]
pub enum WriteError {
    WriteFailed,
}

/// sentry_envelope_t
// PORT NOTE: `pub const Envelope = opaque { ... }` — an opaque C type; only ever
// handled behind `*mut Envelope` raw pointers.
#[repr(C)]
pub struct Envelope {
    _opaque: [u8; 0],
}

impl Envelope {
    // pub fn deinit(self: *Envelope) void {
    /// # Safety
    ///
    /// `self_` must be a live envelope pointer handed to the transport by
    /// sentry-native, not yet freed.
    pub unsafe fn deinit(self_: *mut Envelope) {
        // c.sentry_envelope_free(@ptrCast(self));
        unsafe { c::sentry_envelope_free(self_.cast()) }
    }

    // pub fn writeToFile(self: *Envelope, path: []const u8) !void {
    /// # Safety
    ///
    /// `self_` must be a live envelope pointer (see [`Envelope::deinit`]).
    pub unsafe fn writeToFile(self_: *mut Envelope, path: &str) -> Result<(), WriteError> {
        // if (c.sentry_envelope_write_to_file_n(...) != 0) return error.WriteFailed;
        if unsafe {
            c::sentry_envelope_write_to_file_n(self_.cast(), path.as_ptr().cast(), path.len())
        } != 0
        {
            return Err(WriteError::WriteFailed);
        }
        Ok(())
    }

    // pub fn serialize(self: *Envelope) []u8 {
    /// # Safety
    ///
    /// `self_` must be a live envelope pointer (see [`Envelope::deinit`]). The
    /// returned slice is owned by sentry-native and must be released with
    /// [`super::free`] (`sentry_free`) — exactly the reference contract
    /// (`defer sentry.free(@ptrCast(json.ptr))` at the call site). The
    /// unbounded lifetime mirrors the raw `[]u8` the reference returns.
    pub unsafe fn serialize<'a>(self_: *mut Envelope) -> &'a [u8] {
        // var len: usize = 0;
        let mut len: usize = 0;
        // const ptr = c.sentry_envelope_serialize(@ptrCast(self), &len).?;
        let ptr = unsafe { c::sentry_envelope_serialize(self_.cast(), &mut len) };
        // `.?` — the reference safety-checks the optional.
        assert!(!ptr.is_null());
        // return ptr[0..len];
        unsafe { std::slice::from_raw_parts(ptr.cast::<u8>(), len) }
    }

    // pub fn event(self: *Envelope) ?Value {
    /// # Safety
    ///
    /// `self_` must be a live envelope pointer (see [`Envelope::deinit`]).
    pub unsafe fn event(self_: *mut Envelope) -> Option<Value> {
        // const val: Value = .{ .value = c.sentry_envelope_get_event(@ptrCast(self)) };
        let val = Value {
            value: unsafe { c::sentry_envelope_get_event(self_.cast()) },
        };
        // if (val.isNull()) return null;
        if val.isNull() {
            return None;
        }
        Some(val)
    }
}
