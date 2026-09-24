//! rustty-font
//!
//! PORTS: ghostty src/font/ (Atlas, Collection, SharedGrid, SharedGridSet, CodepointResolver,
//! discovery, embedded, sprite/, opentype/, ...); links freetype/harfbuzz/fontconfig-sys,
//! CoreText; config via DI (break #9).
//!
//! PORT STATUS: in progress
//!
//! Carved leaves-first (R1: every line/comment/test{} ported as the crate is
//! carved, under the acyclicity gate per R4). This crate is the PURE leaf of
//! the D1 three-tier font split and holds: `opentype` (sfnt, head, hhea, os2,
//! post, glyf, svg), `embedded` (test font fixtures), the `terminal_options`
//! `artifact` seam (`options`), `metrics` (font Metrics + Modifier + Key),
//! `glyph` (Glyph + Size + RenderOptions + Constraint), the generated
//! `nerd_font_attributes` codepoint→Constraint table, `atlas` (texture-atlas
//! rect packing), `codepoint_map` (codepoint→descriptor map, discovery
//! descriptor seamed thin), `glyf_rasterize` (+ its PNG test) and `sprite`
//! (Face, canvas, every `draw/` module). The rest of the `font` crate-map key
//! (faces, shapers, Collection, CodepointResolver, DeferredFace, discovery,
//! SharedGrid/SharedGridSet, library, backend and the `main.zig` glue) is
//! ported in `rustty-font-shaper` (R4/D1). (CORRECTION, 2026-09-24, parity
//! tail S-8a: the "ported so far" list here stopped at `codepoint_map`.)
//!
//! Every reference comment of the 50 mapped files is present across the two
//! crates (G3b `--crate font`: 3357/3357, via its R4 relocation map). Why the
//! status is not `done`: a done claim makes G1/G2 diff the reference tests and
//! assertions against THIS crate's `src/` only, where the shaper-tier tests
//! are absent (G1 `--strict`: 89/178 here), so the flip waits for R4
//! relocation support in `_gatelib`'s per-crate Rust scan; see also the
//! backend residual in `rustty-font-shaper`'s header.

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
