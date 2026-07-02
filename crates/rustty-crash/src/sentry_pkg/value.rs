//! rustty-crash::sentry_pkg::value
//!
//! PORTS: ghostty pkg/sentry/value.zig.
//!
//! PORT STATUS: done
#![allow(non_snake_case)]

// const std = @import("std");
// const assert = std.debug.assert;
use super::c;
use super::level::Level;

/// sentry_value_t
#[derive(Clone, Copy)]
pub struct Value {
    /// The underlying value. This is a union that could be represented with
    /// an extern union but I don't want to risk C ABI issues so we wrap it
    /// in a struct.
    pub value: c::sentry_value_t,
}

impl Value {
    // pub fn initMessageEvent(
    //     level: Level,
    //     logger: ?[]const u8,
    //     message: []const u8,
    // ) Value {
    pub fn initMessageEvent(level: Level, logger: Option<&str>, message: &str) -> Value {
        Value {
            value: unsafe {
                c::sentry_value_new_message_event_n(
                    // @intFromEnum(level),
                    level.intFromEnum(),
                    // if (logger) |v| v.ptr else null,
                    match logger {
                        Some(v) => v.as_ptr().cast(),
                        None => std::ptr::null(),
                    },
                    // if (logger) |v| v.len else 0,
                    match logger {
                        Some(v) => v.len(),
                        None => 0,
                    },
                    message.as_ptr().cast(),
                    message.len(),
                )
            },
        }
    }

    pub fn initObject() -> Value {
        Value {
            value: unsafe { c::sentry_value_new_object() },
        }
    }

    pub fn initString(value: &str) -> Value {
        Value {
            value: unsafe { c::sentry_value_new_string_n(value.as_ptr().cast(), value.len()) },
        }
    }

    pub fn initBool(value: bool) -> Value {
        Value {
            // PORT: `@intFromBool(value)`.
            value: unsafe { c::sentry_value_new_bool(std::ffi::c_int::from(value)) },
        }
    }

    pub fn initInt32(value: i32) -> Value {
        Value {
            value: unsafe { c::sentry_value_new_int32(value) },
        }
    }

    pub fn decref(self) {
        unsafe { c::sentry_value_decref(self.value) }
    }

    // PORT: the reference body is `c.sentry_value_incref(self.value);` with a
    // `Value` return type and NO return statement — it is never referenced, so
    // Zig's lazy semantic analysis never compiles the body. The Rust port must
    // compile; returning `self` (the increfed value) is the evident intent.
    pub fn incref(self) -> Value {
        unsafe { c::sentry_value_incref(self.value) };
        self
    }

    pub fn isNull(self) -> bool {
        unsafe { c::sentry_value_is_null(self.value) != 0 }
    }

    /// sentry_value_set_by_key_n
    pub fn set(self, key: &str, value: Value) {
        // _ = c.sentry_value_set_by_key_n(...)
        let _ = unsafe {
            c::sentry_value_set_by_key_n(self.value, key.as_ptr().cast(), key.len(), value.value)
        };
    }

    /// sentry_value_set_by_key_n
    // (PORT: doc comment preserved verbatim from the reference, which repeats
    // the `set_by_key_n` name on `get`; the C call below is `get_by_key_n`.)
    pub fn get(self, key: &str) -> Option<Value> {
        // const val: Value = .{ .value = c.sentry_value_get_by_key_n(...) };
        let val = Value {
            value: unsafe {
                c::sentry_value_get_by_key_n(self.value, key.as_ptr().cast(), key.len())
            },
        };
        // if (val.isNull()) return null;
        if val.isNull() {
            return None;
        }
        Some(val)
    }
}
