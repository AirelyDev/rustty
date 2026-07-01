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
//! fixtures), the `terminal_options` `artifact` seam (`options`), `metrics`
//! (font Metrics + Modifier + Key), `glyph` (Glyph + Size + RenderOptions +
//! Constraint), the generated `nerd_font_attributes` codepoint→Constraint
//! table, `atlas` (texture-atlas rect packing), and `codepoint_map`
//! (codepoint→descriptor map, discovery descriptor seamed thin).

pub mod atlas;
pub mod codepoint_map;
pub mod embedded;
pub mod glyf_rasterize;
pub mod glyph;
pub mod metrics;
pub mod nerd_font_attributes;
pub mod opentype;
pub mod options;
pub mod sprite;
