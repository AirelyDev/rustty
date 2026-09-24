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
//! Every gate examines both crates: `_gatelib.RELOCATIONS` names
//! `crates/rustty-font-shaper/src` as part of this crate key's port (parity
//! tail S-8c). Measured 2026-09-24 with that relocation, every gate a done
//! claim would bind is green for `font`: G1 `--strict` 278/178 with no MISSING
//! name, G2 970/619 with no WEAKENED test, G3, G3b 3357/3357, and D3.
//!
//! Why the status is still not `done`: the reference selects the font backend
//! with a BUILD OPTION (`-Dfont-backend`, `src/build/Config.zig:191-195`), and
//! its CI builds and tests `coretext_freetype` on macOS (`build-macos-freetype`
//! in `.github/workflows/test.yml`). Here `options::BACKEND` is folded to the
//! per-OS default (`Backend::default()`), and the Face/Shaper/Discover/Library
//! types are chosen by `target_os`, not by the backend. So the non-default arms
//! (`loadCoreTextFreetype`, the `coretext_harfbuzz`/`coretext_noshape`/
//! `freetype` selections) exist only as quoted comments. That is an R8
//! build-switch fold, recorded OPEN as NEW-FINDINGS N266.

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
