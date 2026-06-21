//! rustty-config
//!
//! PORTS: ghostty src/config/ minus formatter.zig + key.zig (config.zig, Config, CApi,
//! ClipboardCodepointMap, ErrorList, RepeatableStringMap, Wasm, c_get, command,
//! conditional, edit, file_load, ...); links oniguruma-sys, themes.
//!
//! PORT STATUS: stub
//!
//! Scaffold only — no real logic yet (R1: every line/comment/test{} ported as
//! the crate is carved, leaves-first, under the acyclicity gate per R4).
