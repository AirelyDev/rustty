//! rustty-os::wasm
//!
//! PORT STATUS: stub
//!
//! This file contains helpers for wasm compilation.
//!
//! Minimal compiling stub. Ports the reference's `src/os/wasm.zig` (the
//! `shared_mem` / `alloc` helpers) plus the `src/os/wasm/` subdirectory — every
//! line, comment, and `test {}` is ported here as the module is carved (R1).

// The `src/os/wasm/` subdirectory (log.zig, target.zig).
pub mod log;
pub mod target;
