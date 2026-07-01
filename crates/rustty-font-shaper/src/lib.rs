//! rustty-font-shaper
//!
//! PORTS: ghostty `src/font/` shaping + backend-face layer that sits ABOVE
//! `rustty-vt` + `rustty-font` — the D1 three-tier split (memory
//! `apc-glyf-font-layering-d1`). `rustty-font` stays a PURE parsing/sprite leaf
//! BELOW `rustty-vt`; this crate is the shaper tier that hosts the backend
//! `Face` selector (`face.zig`), the CoreText/FreeType faces, and HarfBuzz
//! shaping, so it may depend on both `rustty-vt` and `rustty-font` plus the
//! `-sys`/objc2 backends.
//!
//! PORT STATUS: F3 scaffold — `face.zig` (the backend selector) ported; the
//! concrete `face/coretext.zig`, `face/freetype.zig`, `face/web_canvas.zig`
//! backend faces and `shape.zig` land in later F3 subtasks.
//!
//! Carved leaves-first (R1: every line/comment/test{} ported as the crate is
//! carved, under the acyclicity gate per R4). Ported so far: `face` (the
//! backend selector — `Face` = the active backend `Face` type; `Options` /
//! `DesiredSize` / `Variation`, `default_dpi`, the `FreetypeLoadFlags` seam).

pub mod face;
// FFI-glue modules backing the FreeType face (thin `unsafe` wrappers over the
// `-sys` crates; see each module's PORT NOTE). Not part of any reference file's
// test contract — the full `pkg/freetype`, `pkg/harfbuzz`, `src/font/Library.zig`
// and `src/stb` ports supersede them.
pub mod ft;
pub mod hb;
pub mod library;
pub mod stb;

// The active backend `Face` type, selected by `face.zig`'s `switch` over the
// build backend (macOS default=coretext, Linux default=fontconfig_freetype).
pub use face::Face;

// PORT NOTE (shared face types): the reference `font` module (`main.zig`)
// re-exports `Metrics` and the glyph `RenderOptions` alongside the face
// `Options`, so the backend `Face` impls and shapers can name them unqualified
// (`Metrics = @import("Metrics.zig")`, `Glyph.RenderOptions`). Those types are
// owned by the PURE `rustty-font` leaf BELOW this tier; re-export them here as
// the shared face types this shaper tier builds on.
pub use rustty_font::glyph::RenderOptions;
pub use rustty_font::metrics::Metrics;
