//! rustty-crash::sentry_pkg
//!
//! PORTS: ghostty pkg/sentry (the Zig wrapper package over sentry-native:
//! main.zig, c.zig, envelope.zig, level.zig, transport.zig, uuid.zig,
//! value.zig).
//!
//! PORT STATUS: done
//!
//! This is DEP-glue in the rustty-z2d / rustty-oniguruma sense: `pkg/sentry`
//! is a dependency *package* of the reference (imported as `@import("sentry")`
//! by src/crash/sentry.zig), not ghostty `src/` code, so it is hosted as a
//! module inside rustty-crash (its only consumer, per the integration-tier
//! blueprint) rather than as its own parity crate. It is built over the
//! vendored `sentry-sys` (R3: the SAME sentry-native 0.7.8 the reference
//! links, with the product's `backend = .breakpad` — SharedDeps.zig; the
//! breakpad client comes from breakpad-sys — and `transport = .none`). The
//! wrapper surface is
//! ported 1:1 from the reference package (every declaration, incl. the ones
//! src/crash does not reach yet — captureEvent/setContext/removeContext).
#![allow(non_snake_case)]

// pub const c = @import("c.zig").c;
pub mod c;

// const transport = @import("transport.zig");
pub mod transport;

pub mod envelope;
pub mod level;
pub mod uuid;
pub mod value;

// pub const Envelope = @import("envelope.zig").Envelope;
pub use envelope::Envelope;
// pub const Level = @import("level.zig").Level;
pub use level::Level;
// pub const Transport = transport.Transport;
pub use transport::Transport;
// pub const Value = @import("value.zig").Value;
pub use value::Value;
// pub const UUID = @import("uuid.zig").UUID;
pub use uuid::UUID;

// pub fn captureEvent(value: Value) ?UUID {
pub fn captureEvent(value: Value) -> Option<UUID> {
    // const uuid: UUID = .{ .value = c.sentry_capture_event(value.value) };
    let uuid = UUID {
        value: unsafe { c::sentry_capture_event(value.value) },
    };
    // if (uuid.isNil()) return null;
    if uuid.isNil() {
        return None;
    }
    Some(uuid)
}

// pub fn setContext(key: []const u8, value: Value) void {
pub fn setContext(key: &str, value: Value) {
    unsafe { c::sentry_set_context_n(key.as_ptr().cast(), key.len(), value.value) }
}

// pub fn removeContext(key: []const u8) void {
pub fn removeContext(key: &str) {
    unsafe { c::sentry_remove_context_n(key.as_ptr().cast(), key.len()) }
}

// pub fn setTag(key: []const u8, value: []const u8) void {
pub fn setTag(key: &str, value: &str) {
    unsafe {
        c::sentry_set_tag_n(
            key.as_ptr().cast(),
            key.len(),
            value.as_ptr().cast(),
            value.len(),
        )
    }
}

// pub fn free(ptr: *anyopaque) void {
/// # Safety
///
/// `ptr` must be a pointer previously allocated by the sentry-native library
/// (e.g. the buffer returned by `sentry_envelope_serialize`), not yet freed.
pub unsafe fn free(ptr: *mut std::ffi::c_void) {
    unsafe { c::sentry_free(ptr) }
}

// test {
//     @import("std").testing.refAllDecls(@This());
// }
#[cfg(test)]
mod tests {
    use super::*;

    /// Faithful analogue of pkg/sentry `main.zig`'s `refAllDecls` test:
    /// reference every public declaration so it is exercised by the test
    /// build. (No live sentry client is touched — `refAllDecls` only forces
    /// semantic analysis in Zig; here the value-API construction mirrors the
    /// sentry-sys KAT's pure value-API usage, no `sentry_init`/transport.)
    #[test]
    fn ref_all_decls() {
        let _: fn(Value) -> Option<UUID> = captureEvent;
        let _: fn(&str, Value) = setContext;
        let _: fn(&str) = removeContext;
        let _: fn(&str, &str) = setTag;
        let _: unsafe fn(*mut std::ffi::c_void) = free;
        let _: Level = Level::Debug;

        // Pure value-API round-trip (the sentry-sys KAT idiom).
        let obj = Value::initObject();
        assert!(!obj.isNull());
        obj.decref();
    }
}
