//! rustty-font opentype
//!
//! PORTS: ghostty src/font/opentype/ (the pure-parsing OpenType table parsers)
//!
//! PORT STATUS: in progress (sfnt, head, glyf, hhea, os2, post, svg)

// PORT NOTE: the reference `opentype.zig` re-exports each parser under a
// capitalized alias (`pub const Head = head.Head;` etc.). Rust reaches them as
// `opentype::head::Head`, so the modules are declared directly; the shaper tier's
// CoreText face names them via these paths.
pub mod glyf;
pub mod head;
pub mod hhea;
pub mod os2;
pub mod post;
pub mod sfnt;
pub mod svg;
