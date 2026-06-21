//! rustty-cli
//!
//! PORTS: ghostty src/cli/ minus args.zig/diagnostics.zig/CommaSplitter.zig (cli.zig, Pager,
//! action, boo, crash_report, edit_config, explain_config, ghostty, help, list_actions,
//! list_colors, list_fonts, ...). CLI command surface renamed to `rustty` (R2).
//!
//! PORT STATUS: stub
//!
//! Scaffold only — no real logic yet (R1: every line/comment/test{} ported as
//! the crate is carved, leaves-first, under the acyclicity gate per R4).
