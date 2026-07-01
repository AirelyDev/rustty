//! rustty-font-shaper::shared_grid — SEAM for ghostty `src/font/SharedGrid.zig`.
//!
//! PORT NOTE (seam, NOT a full port): the reference `SharedGrid.zig` (540 loc)
//! is a shared glyph-atlas grid — a `CodepointResolver` plus two `Atlas` grids,
//! a codepoint→index cache, grid `Metrics`, and an `RwLock` — that the renderer
//! reads (`font_grid`). Its full port is a LATER heavy-tier subtask (it is NOT
//! in the shaper-framework file set: `shape.zig` + `shaper/{run,feature,Cache,
//! noop}.zig`). The shaper framework references `SharedGrid` for exactly three
//! things: `getIndex`, `hasCodepoint`, and reaching `resolver.collection.getFace`
//! (the noop shaper). This seam provides those with faithful *observable*
//! behavior by delegating straight to the owned `CodepointResolver`.
//!
//! The atlas grids, the `codepoints`/`glyphs` caches, the grid `Metrics`, and
//! the `RwLock` are intentionally omitted here: they are pure performance /
//! rendering-surface state that does not change the *result* of `getIndex` /
//! `hasCodepoint` (the cache only memoizes the resolver's answer, the lock only
//! guards concurrent access). They land with the full `SharedGrid.zig` port,
//! which will REPLACE this seam.

// Reference imports (SharedGrid.zig): the resolver it owns + the shared enums.
// const std = @import("std");
// const Allocator = std.mem.Allocator;
// const CodepointResolver = @import("CodepointResolver.zig");
// const Collection = @import("Collection.zig");
// const Style = @import("main.zig").Style;
// const Presentation = @import("main.zig").Presentation;
use allocator_api2::alloc::Allocator;

use crate::codepoint_resolver::CodepointResolver;
use crate::collection::{self, PresentationMode};
use crate::{Presentation, Style};

/// SEAM for `SharedGrid` (see the module doc). Only the fields/methods the
/// shaper framework touches are present; the atlas/cache/metrics/lock state
/// lands with the full `SharedGrid.zig` port.
pub struct SharedGrid {
    /// The underlying resolver for font data, fallbacks, etc. The shared
    /// grid takes ownership of the resolver and will free it.
    // resolver: CodepointResolver,
    pub resolver: CodepointResolver,
}

impl SharedGrid {
    /// Get the font index for a given codepoint, loading the face in the
    /// process (so future callers can rely on the face being loaded).
    ///
    // PORT NOTE (seam): the reference `getIndex` (`*SharedGrid`) first checks a
    // read-locked `codepoints` cache, then under a write lock inserts the
    // resolver's answer (caching even negative matches) and preloads the face.
    // The cache/lock are omitted in this seam; the *observable* result is
    // identical — resolve, then preload the (non-special) face so a later
    // deferred load can't fail unexpectedly.
    // pub fn getIndex(self, alloc, cp, style, p) !?Collection.Index { ... }
    pub fn get_index<A: Allocator + Copy>(
        &mut self,
        alloc: A,
        cp: u32,
        style: Style,
        p: Option<Presentation>,
    ) -> Result<Option<collection::Index>, collection::Error> {
        // Load a value. This even caches negative matches (in the full grid).
        let value = self.resolver.get_index(alloc, cp, style, p);

        if let Some(idx) = value {
            'preload: {
                // If the font is a sprite font then we don't need to preload
                // because getFace doesn't work with special fonts.
                if idx.special().is_some() {
                    break 'preload;
                }

                // Load the face in case its deferred. If this fails then we
                // would've failed to load it in the future anyways so we want to
                // undo all the caching we did.
                let _ = self.resolver.collection.get_face(idx)?;
            }
        }

        Ok(value)
    }

    /// Returns true if the given font index has the codepoint and presentation.
    // pub fn hasCodepoint(self, idx, cp, p) bool {
    //     return self.resolver.collection.hasCodepoint(
    //         idx, cp, if (p) |v| .{ .explicit = v } else .{ .any = {} });
    // }
    pub fn has_codepoint(&self, idx: collection::Index, cp: u32, p: Option<Presentation>) -> bool {
        self.resolver.collection.has_codepoint(
            idx,
            cp,
            match p {
                Some(v) => PresentationMode::Explicit(v),
                None => PresentationMode::Any,
            },
        )
    }
}
