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
//! PORT STATUS: ported (the FULL D1 shaper tier — 110 tests; the old
//! "F3 — shape.zig lands later" sentence was stale doc drift, realigned per
//! VALIDATION_REPORT.md §3b): `face.zig` (the backend selector) + the
//! `face/freetype.zig` and `face/coretext.zig` (macOS) backend faces,
//! `shape.zig` + the `shaper/` HarfBuzz framework (`run`/`feature`/`Cache`/
//! `noop`), `discovery.zig` (CoreText discoverer + `Descriptor`),
//! `Collection.zig`/`CodepointResolver.zig`, and `SharedGrid.zig`/
//! `SharedGridSet.zig`. Honest residual (macOS-first locked fork — DEFERRALS.md
//! "rustty-font-shaper"): the 3 Fontconfig `DeferredFace` arms
//! (`deferred_face.rs`, Linux backend) + the non-macOS Fontconfig discovery
//! half, and `face/web_canvas.zig` (wasm browser face, unported).
//!
//! Carved leaves-first (R1: every line/comment/test{} ported as the crate is
//! carved, under the acyclicity gate per R4).

pub mod backend;
pub mod codepoint_resolver;
pub mod collection;
// Local DI stand-ins for the `rustty-config` value types `SharedGridSet` reads
// (break #9: config is a HIGHER crate injected via DI; see the module doc).
pub mod config;
pub mod deferred_face;
pub mod discovery;
pub mod face;
// PORT NOTE: `shape.zig` (the `Shaper` interface + backend selector) and its
// `shaper/` sub-files (`run`/`feature`/`Cache`/`noop`) — the shaper framework.
pub mod shape;
// PORT NOTE: `SharedGrid.zig` (the shared glyph-atlas grid the renderer reads —
// `renderGlyph` caching over Collection+Atlas) and `SharedGridSet.zig` (the set
// keyed by config/font descriptors).
pub mod shared_grid;
pub mod shared_grid_set;
// FFI-glue modules backing the FreeType face (thin `unsafe` wrappers over the
// `-sys` crates; see each module's PORT NOTE). Not part of any reference file's
// test contract — the full `pkg/freetype`, `pkg/harfbuzz`, `src/font/Library.zig`
// and `src/stb` ports supersede them.
pub mod ft;
pub mod hb;
pub mod library;
pub mod stb;
// `uucode` seam (the single `is_emoji_presentation` property `CodepointResolver`
// reads). Glue, superseded by a full unicode-database port later.
pub mod uucode;

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

// ---------------------------------------------------------------------------
// PORT NOTE: the `main.zig`-level declarations of the `font` module. `main.zig`
// re-exports the sub-files as one namespace and defines the small shared enums,
// the build `options`, and the fallback constants. In rustty the pure leaf
// (`rustty-font`) owns `Atlas`/`Glyph`/`Metrics`/`CodepointMap`/`sprite`/
// `opentype`/`embedded`; this shaper tier owns `face`/`Collection`/`DeferredFace`/
// `CodepointResolver`/`Library`/`discovery`. The `main.zig` glue below wires them
// together (the `font.*` names the ported files reference).
// ---------------------------------------------------------------------------

// pub const Atlas = @import("Atlas.zig");
pub use rustty_font::atlas::Atlas;
// pub const Backend = @import("backend.zig").Backend;
pub use backend::Backend;
// pub const CodepointMap = @import("CodepointMap.zig");
pub use rustty_font::codepoint_map::CodepointMap;
// pub const CodepointResolver = @import("CodepointResolver.zig");
pub use codepoint_resolver::CodepointResolver;
// pub const Collection = @import("Collection.zig");
pub use collection::Collection;
// pub const DeferredFace = @import("DeferredFace.zig");
pub use deferred_face::DeferredFace;
// pub const Glyph = @import("Glyph.zig");
pub use rustty_font::glyph::Glyph;
// pub const Metrics = @import("Metrics.zig"); — re-exported above.
// pub const Descriptor = discovery.Descriptor;
pub use discovery::Descriptor;
// pub const Discover = discovery.Discover;
pub use discovery::Discover;
// pub const Library = library.Library;
pub use library::Library;
// pub const Shaper = shape.Shaper;
pub use shape::Shaper;
// pub const ShaperCache = shape.Cache;
pub use shape::Cache as ShaperCache;
// pub const SharedGrid = @import("SharedGrid.zig");
pub use shared_grid::SharedGrid;
// pub const SharedGridSet = @import("SharedGridSet.zig");
pub use shared_grid_set::SharedGridSet;
// pub const SpriteFace = sprite.Face;
pub use rustty_font::sprite::Face as SpriteFace;
// pub const Sprite = sprite.Sprite;
pub use rustty_font::sprite::Sprite;

/// Build options
// pub const options: struct { backend: Backend } = .{ .backend = ... };
//
// PORT NOTE: the reference `options` is an anonymous struct with one `backend`
// field, set from `build_config.font_backend` (or `.web_canvas` for wasm). Here
// it is a module holding the `BACKEND` const, resolved from `cfg` via
// `Backend::default()` (the same decision `build.zig` makes). Parity targets both
// defaults (macOS `coretext`, Linux `fontconfig_freetype`).
pub mod options {
    use crate::backend::Backend;

    /// The active font backend for this build.
    pub const BACKEND: Backend = Backend::default();
}

/// The styles that a family can take.
// pub const Style = enum(u3) { regular=0, bold=1, italic=2, bold_italic=3 };
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Style {
    Regular = 0,
    Bold = 1,
    Italic = 2,
    BoldItalic = 3,
}

/// The presentation for an emoji.
// pub const Presentation = enum(u1) { text=0 /*U+FE0E*/, emoji=1 /*U+FE0F*/ };
// PORT NOTE: `Hash` is derived (beyond the reference's plain enum) so
// `Presentation` can be a field of `SharedGrid`'s `CodepointKey` hash-map key
// (the reference's `AutoHashMapUnmanaged` auto-derives key hashing).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Presentation {
    /// U+FE0E
    Text = 0,
    /// U+FE0F
    Emoji = 1,
}

/// A FontIndex that can be used to use the sprite font directly.
// pub const sprite_index = Collection.Index.initSpecial(.sprite);
pub const SPRITE_INDEX: collection::Index =
    collection::Index::init_special(collection::index::Special::Sprite);

/// The default font size adjustment we use when loading fallback fonts.
///
/// TODO: Add user configuration for this instead of hard-coding it.
// pub const default_fallback_adjustment: Collection.SizeAdjustment = .ic_width;
pub const DEFAULT_FALLBACK_ADJUSTMENT: collection::SizeAdjustment =
    collection::SizeAdjustment::IcWidth;
