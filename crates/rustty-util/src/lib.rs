//! rustty-util
//!
//! PORTS: ghostty src/ files: math.zig, fastmem.zig, file_type.zig, tripwire.zig, quirks.zig (the assert/util leaf).
//!
//! PORT STATUS: done
//!
//! This crate is a leaf: it has NO dependency on rustty-font (R3/R4). The one
//! font-dependent quirk (`disableDefaultFontFeatures`) is exposed as a trait
//! seam (`quirks::Face`) so the actual face type can be wired in when
//! rustty-font lands, without pulling font into this Miri-able leaf.

pub mod fastmem;
pub mod file_type;
pub mod math;
pub mod quirks;
pub mod tripwire;
