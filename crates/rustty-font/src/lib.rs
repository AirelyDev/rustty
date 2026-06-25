//! rustty-font
//!
//! PORTS: ghostty src/font/ (Atlas, Collection, SharedGrid, SharedGridSet, CodepointResolver,
//! discovery, embedded, sprite/, opentype/, ...); links freetype/harfbuzz/fontconfig-sys,
//! CoreText; config via DI (break #9).
//!
//! PORT STATUS: in progress
//!
//! Carved leaves-first (R1: every line/comment/test{} ported as the crate is
//! carved, under the acyclicity gate per R4). Ported so far: the pure-parsing
//! OpenType slice (`opentype::{sfnt, head, glyf}`), `embedded` (test font
//! fixtures), and the `terminal_options` `artifact` seam (`options`).

pub mod embedded;
pub mod opentype;
pub mod options;
