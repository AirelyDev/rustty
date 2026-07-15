//! rustty-os::wasm
//!
//! PORT STATUS: ported
//!
//! This file contains helpers for wasm compilation.
//!
//! Ports the reference's `src/os/wasm.zig` (the `shared_mem` / `alloc` helpers)
//! plus the `src/os/wasm/` subdirectory — every line, comment, and `test {}` is
//! ported here (R1). Retargeted only for product identity (R2).

// PORT NOTE: the Zig reference imports `std`, `builtin`, `options = build_options`,
// and has a `comptime` guard `@compileError("wasm.zig should only be analyzed
// for wasm32 builds")`. In Rust the equivalent is a `cfg(target_arch = "wasm32")`
// gate on the module body so it is only compiled for wasm; the guard message is
// preserved verbatim below.
#![allow(dead_code)]

// The `src/os/wasm/` subdirectory (log.zig, target.zig).
pub mod log;
pub mod target;

// PORT NOTE: `comptime { if (!builtin.target.cpu.arch.isWasm()) @compileError("wasm.zig
// should only be analyzed for wasm32 builds"); }`. The whole `wasm_impl` module is
// `cfg(target_arch = "wasm32")`, which is the parity-preserving equivalent: the
// declarations below are only compiled for wasm32 builds.
#[cfg(target_arch = "wasm32")]
// PORT: the reference consts are `shared_mem`/`alloc` (snake_case Zig names);
// R1 preserves those identifiers verbatim, so allow the non-UPPER_CASE lint.
#[allow(non_upper_case_globals)]
mod wasm_impl {
    // wasm.zig should only be analyzed for wasm32 builds

    /// True if we're in shared memory mode. If true, then the memory buffer
    /// in JS will be backed by a SharedArrayBuffer and some behaviors change.
    // PORT: `pub const shared_mem = options.wasm_shared;`. `rustty_build_config`
    // exposes it via the `wasm_shared()` accessor (the `build_config.zig`
    // re-export analog; see the sibling `font_backend()`/`renderer()` accessors).
    pub const shared_mem: bool = rustty_build_config::wasm_shared();

    /// The allocator to use in wasm environments.
    ///
    /// The return values of this should NOT be sent to the host environment
    /// unless toHostOwned is called on them. In this case, the caller is expected
    /// to call free. If a pointer is NOT host-owned, then the wasm module is
    /// expected to call the normal alloc.free/destroy functions.
    // PORT NOTE: `pub const alloc = if (builtin.is_test) std.testing.allocator else
    // std.heap.wasm_allocator;`. The Rust global allocator fulfills both roles
    // (the test allocator's leak-checking is covered by Miri per R6); `alloc`
    // names the wasm heap allocator used in wasm environments.
    pub use std::alloc::System as alloc;
}

#[cfg(target_arch = "wasm32")]
pub use wasm_impl::{alloc, shared_mem};
