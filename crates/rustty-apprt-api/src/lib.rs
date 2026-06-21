//! rustty-apprt-api
//!
//! PORTS: ghostty apprt interface seam (action.zig types + surface target/message traits)
//! extracted from src/apprt/ as a pure interface crate (break #8, breaks apprt<->core).
//! No whole reference file moves intact; this is the trait seam (ambiguity A5).
//!
//! PORT STATUS: stub
//!
//! Scaffold only — no real logic yet (R1: every line/comment/test{} ported as
//! the crate is carved, leaves-first, under the acyclicity gate per R4).
