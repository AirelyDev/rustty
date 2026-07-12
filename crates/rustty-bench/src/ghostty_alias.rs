//! `ghostty_benchmark_cli` C-ABI alias layer (Branch A) — GENERATED for the macOS
//! Swift-app tier.
//!
//! A single thin `#[no_mangle]` forwarder `ghostty_benchmark_cli` that calls the
//! canonical `rustty_benchmark_cli` (`crate::capi::rustty_benchmark_cli`) with
//! byte-identical arguments. This gives the linked staticlib BOTH symbol spellings
//! from one body so the reused upstream Ghostty Swift app (which hard-codes
//! `ghostty_benchmark_cli`) links unchanged, while `rustty_benchmark_cli` stays the
//! canonical rustty-native ABI (R2). PREPORT_SWIFT_APP §3 Branch A, mechanism (a)
//! (thin wrapper; staticlib + xcframework-safe, avoids the linker-alias /
//! `objcopy --add-symbol` portability risk). Compiled only under the
//! `identity-ghostty` cargo feature.
//!
//! GENERATED from the sole `rustty_benchmark_cli` decl in `capi.rs` — this is the
//! `benchmark` slice of the 90-symbol app C-ABI (`src/benchmark/CApi.zig`'s one
//! export). It lives here (not in apprt-c) because its body lives here; `apprt-c`
//! (the `main_c.zig` hub analog, whose `_ = @import("benchmark/main.zig").CApi`
//! reference force-links this crate) enables this crate's `identity-ghostty` feature
//! from its own so the alias lands in the staticlib. The `rustty-config` per-crate
//! alias file is the precedent.

#![allow(clippy::missing_safety_doc)]

use core::ffi::c_char;

// export fn ghostty_benchmark_cli(action_name_: [*:0]const u8, args: [*:0]const u8) bool
#[no_mangle]
pub unsafe extern "C" fn ghostty_benchmark_cli(
    action_name: *const c_char,
    args: *const c_char,
) -> bool {
    // SAFETY: forwards the caller contract unchanged to the canonical rustty_* export.
    unsafe { crate::capi::rustty_benchmark_cli(action_name, args) }
}
