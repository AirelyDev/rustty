//! rustty-vt-c
//!
//! PORTS: ghostty src/terminal/c/ (terminal, render, kitty_graphics, selection_gesture, ...) —
//! the C ABI layer (break #4). cdylib+staticlib+rlib; cbindgen -> include/rustty.h, rustty_* symbols.
//!
//! PORT STATUS: stub
//!
//! Scaffold only — no real ported logic yet (R1: every line/comment/test{} ported
//! as the crate is carved, leaves-first, under the acyclicity gate per R4).
//!
//! The single exported symbol below exists ONLY to prove the cbindgen toolchain
//! end-to-end (it generates a real `rustty_*`-prefixed declaration in
//! `include/rustty.h`). It is replaced by the ported `terminal/c/` ABI surface
//! when break #4 lands. Per R2 the published C symbols carry the `rustty_`
//! prefix and the header is `include/rustty.h` (+ `rustty/vt/*`).

use std::os::raw::c_int;

/// Returns the ABI version of the rustty-vt C interface.
///
/// Toolchain-proof stub: gives cbindgen a `#[no_mangle] extern "C"` symbol to
/// emit so the header pipeline can be validated before the real ABI is ported.
/// The `rustty_` prefix is applied by cbindgen (see `cbindgen.toml`).
///
/// # Safety
/// This function has no preconditions; it is `extern "C"` only to participate
/// in the C ABI.
#[no_mangle]
pub extern "C" fn rustty_vt_abi_version() -> c_int {
    0
}
