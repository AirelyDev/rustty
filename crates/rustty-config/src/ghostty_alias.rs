//! `ghostty_config_*` C-ABI alias layer (Branch A) — GENERATED for the macOS Swift-app tier.
//!
//! Each entry is a thin `#[no_mangle]` forwarder `ghostty_config_<name>` that calls the
//! canonical `rustty_config_<name>` (`crate::c_api::rustty_config_*`) with byte-identical
//! arguments. This gives the linked staticlib BOTH symbol spellings from one body so the
//! reused upstream Ghostty Swift app (which hard-codes `ghostty_config_*`) links unchanged,
//! while `rustty_config_*` stays the canonical rustty-native ABI (R2). PREPORT_SWIFT_APP §3
//! Branch A, mechanism (a) (thin wrappers; staticlib + xcframework-safe, avoids the
//! linker-alias / `objcopy --add-symbol` portability risk). Compiled only under the
//! `identity-ghostty` cargo feature.
//!
//! GENERATED from the `rustty_config_*` decls in `c_api.rs` — one forwarder per `Config`
//! C-ABI export DEFINED in this crate. These are the `config` slice of the 90-symbol app
//! C-ABI; they live here (not in apprt-c) because their bodies live here. `apprt-c` enables
//! this crate's `identity-ghostty` feature from its own so the aliases land in the staticlib.

#![allow(clippy::missing_safety_doc)]

use crate::c_api::{Diagnostic, String};
use crate::config_struct::Config;
use rustty_input::binding::TriggerC;

#[no_mangle]
pub extern "C" fn ghostty_config_new() -> *mut Config {
    crate::c_api::rustty_config_new()
}
#[no_mangle]
pub unsafe extern "C" fn ghostty_config_free(ptr: *mut Config) {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_config_* export.
    unsafe { crate::c_api::rustty_config_free(ptr) }
}
#[no_mangle]
pub unsafe extern "C" fn ghostty_config_clone(self_: *mut Config) -> *mut Config {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_config_* export.
    unsafe { crate::c_api::rustty_config_clone(self_) }
}
#[no_mangle]
pub unsafe extern "C" fn ghostty_config_load_cli_args(self_: *mut Config) {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_config_* export.
    unsafe { crate::c_api::rustty_config_load_cli_args(self_) }
}
#[no_mangle]
pub unsafe extern "C" fn ghostty_config_load_default_files(self_: *mut Config) {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_config_* export.
    unsafe { crate::c_api::rustty_config_load_default_files(self_) }
}
#[no_mangle]
pub unsafe extern "C" fn ghostty_config_load_file(
    self_: *mut Config,
    path: *const std::os::raw::c_char,
) {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_config_* export.
    unsafe { crate::c_api::rustty_config_load_file(self_, path) }
}
#[no_mangle]
pub unsafe extern "C" fn ghostty_config_load_recursive_files(self_: *mut Config) {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_config_* export.
    unsafe { crate::c_api::rustty_config_load_recursive_files(self_) }
}
#[no_mangle]
pub unsafe extern "C" fn ghostty_config_finalize(self_: *mut Config) {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_config_* export.
    unsafe { crate::c_api::rustty_config_finalize(self_) }
}
#[no_mangle]
pub unsafe extern "C" fn ghostty_config_get(
    self_: *mut Config,
    ptr: *mut std::ffi::c_void,
    key_str: *const u8,
    len: usize,
) -> bool {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_config_* export.
    unsafe { crate::c_api::rustty_config_get(self_, ptr, key_str, len) }
}
// The faithful `Trigger.C` projection returns `rustty_input`'s `TriggerC`; its C-layout
// unification belongs to rustty-input (see the `rustty_config_trigger` decl), so the
// `improper_ctypes_definitions` lint is allowed locally on the forwarder too.
#[allow(improper_ctypes_definitions)]
#[no_mangle]
pub unsafe extern "C" fn ghostty_config_trigger(
    self_: *mut Config,
    str_: *const u8,
    len: usize,
) -> TriggerC {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_config_* export.
    unsafe { crate::c_api::rustty_config_trigger(self_, str_, len) }
}
#[no_mangle]
pub unsafe extern "C" fn ghostty_config_diagnostics_count(self_: *mut Config) -> u32 {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_config_* export.
    unsafe { crate::c_api::rustty_config_diagnostics_count(self_) }
}
#[no_mangle]
pub unsafe extern "C" fn ghostty_config_get_diagnostic(self_: *mut Config, idx: u32) -> Diagnostic {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_config_* export.
    unsafe { crate::c_api::rustty_config_get_diagnostic(self_, idx) }
}
#[no_mangle]
pub extern "C" fn ghostty_config_open_path() -> String {
    crate::c_api::rustty_config_open_path()
}
