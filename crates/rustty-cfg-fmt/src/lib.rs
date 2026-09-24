//! rustty-cfg-fmt
//!
//! PORTS: ghostty src/config/formatter.zig — leaf-ish config formatting base
//! extracted below both config and input (break #6, breaks config<->input).
//!
//! PORT STATUS: done
//!
//! Every line/comment/test{} of BOTH mapped reference files is ported (R1):
//! `formatter.zig` in `formatter.rs` here, and `config/key.zig` — the `Key`
//! enum, the `Value` fn and its `test "Value"` — in
//! `rustty-config/src/config_struct.rs`, next to the `Config` struct they
//! reflect over (R4: `formatter.zig`'s `key.zig` import names `Key` only on the
//! import line — it is DEAD code in this module — so `Key` is not pulled in
//! here, keeping this crate a near-leaf under the acyclicity gate). (CORRECTION,
//! 2026-09-24, parity tail S-8a: this header called `key.zig` "DEFERRED"; it
//! was ported with `Config`.)
//!
//! Every gate examines both files: `_gatelib.RELOCATIONS` names key.zig's three
//! items inside `config_struct.rs` (`Key`, `value_type`, the `key_tests`
//! module), so G1/G2/G3/G3b/D3 scan exactly those items as this crate's port,
//! and nothing else of that file. (Status `in progress` -> `done`, 2026-09-24,
//! parity tail S-8c, once the relocation support landed and every gate's
//! default run was green for this crate.)

pub mod formatter;
pub mod writer;

pub use formatter::{
    entry_formatter, format_entry_display, format_entry_enum, format_entry_float,
    format_entry_optional, format_entry_packed_struct, format_entry_string, format_entry_void,
    EntryFormatter, FloatEntry, FormatEntry, FormatError,
};
pub use writer::{AllocatingWriter, Writer};
