//! rustty-crash::sentry_pkg::uuid
//!
//! PORTS: ghostty pkg/sentry/uuid.zig.
//!
//! PORT STATUS: done
#![allow(non_snake_case)]

// const std = @import("std");
// const assert = std.debug.assert;
use super::c;

/// sentry_uuid_t
#[derive(Clone, Copy)]
pub struct UUID {
    pub value: c::sentry_uuid_t,
}

impl UUID {
    // pub fn init() UUID {
    pub fn init() -> UUID {
        // return .{ .value = c.sentry_uuid_new_v4() };
        UUID {
            value: unsafe { c::sentry_uuid_new_v4() },
        }
    }

    // pub fn isNil(self: UUID) bool {
    pub fn isNil(&self) -> bool {
        // return c.sentry_uuid_is_nil(&self.value) != 0;
        unsafe { c::sentry_uuid_is_nil(&self.value) != 0 }
    }

    // pub fn string(self: UUID) [36:0]u8 {
    // PORT: Zig's `[36:0]u8` is a 36-char array with a NUL sentinel — a 37-byte
    // buffer (`sentry_uuid_as_string` expects `char[37]`). The 36 UUID
    // characters are `buf[..36]`.
    pub fn string(&self) -> [u8; 37] {
        // var buf: [36:0]u8 = undefined;
        let mut buf = [0u8; 37];
        // c.sentry_uuid_as_string(&self.value, &buf);
        unsafe { c::sentry_uuid_as_string(&self.value, buf.as_mut_ptr().cast()) };
        buf
    }
}
