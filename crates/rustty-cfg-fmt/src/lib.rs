//! rustty-cfg-fmt
//!
//! PORTS: ghostty src/config/formatter.zig — leaf-ish config formatting base
//! extracted below both config and input (break #6, breaks config<->input).
//!
//! PORT STATUS: in progress
//!
//! Every line/comment/test{} of BOTH mapped reference files is ported (R1):
//! `formatter.zig` in `formatter.rs` here, and `config/key.zig` — the `Key`
//! enum, the `Value` fn and its `test "Value"` — in
//! `rustty-config/src/config_struct.rs`, next to the `Config` struct they
//! reflect over (R4: `formatter.zig`'s `key.zig` import names `Key` only on the
//! import line — it is DEAD code in this module — so `Key` is not pulled in
//! here, keeping this crate a near-leaf under the acyclicity gate). G3b binds
//! both files via its R4 relocation map (`--crate cfg-fmt`: 13/13 reference
//! comments). (CORRECTION, 2026-09-24, parity tail S-8a: this header called
//! `key.zig` "DEFERRED"; it was ported with `Config`.)
//!
//! Why the status is not `done`: a done claim makes G1/G2 diff the reference
//! tests against THIS crate's `src/` only, and `test "Value"` lives in
//! rustty-config, so G1 would report `cfg-fmt::Value` MISSING. The flip waits
//! for R4 relocation support in `_gatelib`'s per-crate Rust scan (the G3b map
//! in `check_comment_coverage.py` is the model).

pub mod formatter;
pub mod writer;

pub use formatter::{
    entry_formatter, format_entry_display, format_entry_enum, format_entry_float,
    format_entry_optional, format_entry_packed_struct, format_entry_string, format_entry_void,
    EntryFormatter, FloatEntry, FormatEntry, FormatError,
};
pub use writer::{AllocatingWriter, Writer};
