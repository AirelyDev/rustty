//! rustty-cfg-fmt
//!
//! PORTS: ghostty src/config/formatter.zig — leaf-ish config formatting base
//! extracted below both config and input (break #6, breaks config<->input).
//!
//! PORT STATUS: stub
//!
//! Carved incrementally (like `rustty-vt`): `formatter.zig` is FULLY ported in
//! `formatter.rs` (PORT STATUS: ported — every line/comment/test{}, R1). The
//! crate-level status stays `stub` because the crate's other mapped reference
//! file, `config/key.zig`, is DEFERRED: `formatter.zig`'s `key.zig` import names
//! `Key` only on the import line — it is DEAD code in this module (`Key` lives
//! with `Config` later), so it is NOT pulled in here, keeping this crate a
//! near-leaf under the acyclicity gate (R4). The formatter sub-batch is gated by
//! the scoped-G3b run against `formatter.zig` only.

pub mod formatter;
pub mod writer;

pub use formatter::{
    entry_formatter, format_entry_display, format_entry_enum, format_entry_float,
    format_entry_optional, format_entry_packed_struct, format_entry_string, format_entry_void,
    EntryFormatter, FloatEntry, FormatEntry, FormatError,
};
pub use writer::{AllocatingWriter, Writer};
