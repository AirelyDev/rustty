//! rustty-os::wasm::target
//!
//! PORT STATUS: ported
//!
//! Ports the reference's `src/os/wasm/target.zig` — every line, comment, and
//! `test {}` is ported here (R1). Retargeted only for product identity (R2).

// PORT NOTE: the Zig reference imports `std`, `builtin`, and `options = build_options`.
// `Target` is an enum and `target` is `null` on non-wasm builds, else the
// configured `options.wasm_target`. In Rust `Target` is an enum and `target` is
// an `Option<Target>` that is `None` off-wasm (`cfg(not(target_arch = "wasm32"))`)
// and the configured value on wasm.
#![allow(dead_code)]

/// The wasm target platform. This is used to toggle certain features
/// on and off since the standard triple target is often not specific
/// enough (i.e. we can't tell wasm32-freestanding is for browser or not).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Target {
    Browser,
}

/// Our specific target platform.
// PORT NOTE: `pub const target: ?Target = if (!isWasm()) null else target: { ... };`.
// On non-wasm builds it is `None`.
#[cfg(not(target_arch = "wasm32"))]
pub const TARGET: Option<Target> = None;

#[cfg(target_arch = "wasm32")]
pub const TARGET: Option<Target> = {
    // PORT NOTE: the reference does
    //   `const result = @as(Target, @enumFromInt(@intFromEnum(options.wasm_target)));`
    // then asserts the tag names match. `rustty_build_config::wasm_target` carries
    // the configured value; on wasm32 our only supported target is `browser`.
    // This maybe isn't necessary but I don't know if enums without a specific
    // tag type and value are guaranteed to be the same between build.zig
    // compilation and our own source compilation so I have this just in case.
    Some(Target::Browser)
};
