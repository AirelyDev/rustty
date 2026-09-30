//! This structure represents the state required to render a terminal
//! grid using the font subsystem. It is "shared" because it is able to
//! be shared across multiple surfaces.
//!
//! It is desirable for the grid state to be shared because the font
//! configuration for a set of surfaces is almost always the same and
//! font data is relatively memory intensive. Further, the font subsystem
//! should be read-heavy compared to write-heavy, so it handles concurrent
//! reads well. Going even further, the font subsystem should be very rarely
//! read at all since it should only be necessary when the grid actively
//! changes.
//!
//! SharedGrid does NOT support resizing, font-family changes, font removals
//! in collections, etc. Because the Grid is shared this would cause a
//! major disruption in the rendering of multiple surfaces (i.e. increasing
//! the font size in one would increase it in all). In many cases this isn't
//! desirable so to implement configuration changes the grid should be
//! reinitialized and all surfaces should switch over to using that one.
// const SharedGrid = @This();

// const std = @import("std");
// const assert = @import("../quirks.zig").inlineAssert;
// const tripwire = @import("../tripwire.zig");
// const Allocator = std.mem.Allocator;
// const renderer = @import("../renderer.zig");
// const font = @import("main.zig");
// const Atlas = font.Atlas;
// const CodepointResolver = font.CodepointResolver;
// const Collection = font.Collection;
// const Face = font.Face;
// const Glyph = font.Glyph;
// const Library = font.Library;
// const Metrics = font.Metrics;
// const Presentation = font.Presentation;
// const Style = font.Style;
// const RenderOptions = font.Glyph.RenderOptions;
//
// PORT NOTE (imports): `renderer.CellSize` is `rustty_render_types::CellSize`
// (the render-types leaf). `tripwire` is `rustty_util::tripwire`. The `font.*`
// types map to the `main.zig` glue re-exports in `lib.rs`. `Atlas` is generic
// over the allocator `A` in rustty (`Atlas<A>`), so `SharedGrid` is likewise
// generic over `A`; the reference threads a bare `Allocator`.
use crate::codepoint_resolver::CodepointResolver;
use crate::collection::{self, PresentationMode};
use crate::{Presentation, Style};
use allocator_api2::alloc::Allocator;
use rustty_font::atlas::{Atlas, Format};
use rustty_font::glyph::{Glyph, RenderOptions};
use rustty_font::metrics::Metrics;
use rustty_render_types::CellSize;
use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hash, Hasher};
use std::sync::RwLock;

// const log = std.log.scoped(.font_shared_grid);

/// SharedGrid: see the module doc comment.
// PORT NOTE: the reference is a file-struct with the fields below; rustty models
// it as a struct generic over the allocator `A` that its two `Atlas` grids use.
pub struct SharedGrid<A: Allocator + Copy = allocator_api2::alloc::Global> {
    /// Cache for codepoints to font indexes in a group.
    // codepoints: std.HashMapUnmanaged(CodepointKey, ?Collection.Index, CodepointKey.Context, 80) = .{},
    codepoints: HashMap<CodepointKey, Option<collection::Index>, PackedKeyState>,

    /// Cache for glyph renders into the atlas.
    // glyphs: std.HashMapUnmanaged(GlyphKey, Render, GlyphKey.Context, 80) = .{},
    glyphs: HashMap<GlyphKey, Render, PackedKeyState>,

    /// The texture atlas to store renders in. The Glyph data in the glyphs
    /// cache is dependent on the atlas matching.
    atlas_grayscale: Atlas<A>,
    atlas_color: Atlas<A>,

    /// The underlying resolver for font data, fallbacks, etc. The shared
    /// grid takes ownership of the resolver and will free it.
    pub resolver: CodepointResolver,

    /// The currently active grid metrics dictating the layout of the grid.
    /// This is calculated based on the resolver and current fonts.
    pub metrics: Metrics,

    /// The RwLock used to protect the shared grid. Callers are expected to use
    /// this directly if they need to i.e. access the atlas directly. Because
    /// callers can use this lock directly, maintainers need to be extra careful
    /// to review call sites to ensure they are using the lock correctly.
    // PORT NOTE: the reference field `lock: RwLock` is public — its doc says
    // "Callers are expected to use this directly". The HarfBuzz shaper's `shape`
    // takes the exclusive lock (`run.grid.lock.lock()`) before touching the
    // freetype/coretext face, so it is `pub` here too.
    pub lock: RwLock<()>,
}

/// The error set for `init` (`Atlas.init` / capacity reserve / `reloadMetrics`).
#[derive(Debug)]
pub enum InitError {
    OutOfMemory,
    Collection(collection::Error),
}

/// `@errorName` (NEW-FINDINGS N121).
impl rustty_zigstd::error_name::ErrorName for InitError {
    fn error_name(&self) -> std::borrow::Cow<'static, str> {
        match self {
            InitError::OutOfMemory => std::borrow::Cow::Borrowed("OutOfMemory"),
            InitError::Collection(e) => e.error_name(),
        }
    }
}

impl From<collection::Error> for InitError {
    fn from(e: collection::Error) -> Self {
        InitError::Collection(e)
    }
}

// FIXME: This gets a massive manual error set for now because there is an
// equally massive call stack in the get_presentation tracepoint. This should
// be consolidated into something better ASAP.
//
/// The error set for `renderGlyph`/`renderCodepoint` (tripwire-injected
/// `OutOfMemory`, the resolver errors, and the atlas-grow `OutOfMemory`).
#[derive(Debug)]
pub enum RenderError {
    OutOfMemory,
    Resolver(crate::codepoint_resolver::Error),
    Collection(collection::Error),
}

impl From<crate::codepoint_resolver::Error> for RenderError {
    fn from(e: crate::codepoint_resolver::Error) -> Self {
        RenderError::Resolver(e)
    }
}

impl From<collection::Error> for RenderError {
    fn from(e: collection::Error) -> Self {
        RenderError::Collection(e)
    }
}

// PORT NOTE (tripwire): the reference declares module-level `init_tw` and
// `renderGlyph_tw` via `tripwire.module(enum {...}, fn)`. rustty's
// `rustty_util::tripwire::Module<P, E>` is an INSTANCE (not a type), so — as in
// `rustty-vt`'s `tabstops` — each module lives in a `#[cfg(test)] thread_local!`
// and the `check` call sites in `init`/`renderGlyph` are `#[cfg(test)]`-gated
// (production never reaches `check`, matching the Zig `enabled = builtin.is_test`
// gate). The fail-point enums are `InitFp` / `RenderGlyphFp` below.
//
// PORT NOTE: `tripwire::Module<P, E>` requires `E: Copy`; the reference injects
// only `error.OutOfMemory`, so the modules carry the small `Copy` marker `TwOom`
// and each `*_tw_check` maps it into the function's (non-`Copy`) error set.
#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TwOom {
    OutOfMemory,
}

// pub const init_tw = tripwire.module(enum {
//     codepoints_capacity,
//     glyphs_capacity,
//     reload_metrics,
// }, init);
#[cfg(test)]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) enum InitFp {
    CodepointsCapacity,
    GlyphsCapacity,
    ReloadMetrics,
}

#[cfg(test)]
thread_local! {
    pub(crate) static INIT_TW:
        core::cell::RefCell<rustty_util::tripwire::Module<InitFp, TwOom>> =
        core::cell::RefCell::new(rustty_util::tripwire::module());
}

#[cfg(test)]
#[inline]
fn init_tw_check(point: InitFp) -> Result<(), InitError> {
    INIT_TW
        .with(|tw| tw.borrow_mut().check(point))
        .map_err(|_| InitError::OutOfMemory)
}

// pub const renderGlyph_tw = tripwire.module(enum {
//     get_presentation,
// }, renderGlyph);
#[cfg(test)]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) enum RenderGlyphFp {
    GetPresentation,
}

#[cfg(test)]
thread_local! {
    pub(crate) static RENDER_GLYPH_TW:
        core::cell::RefCell<rustty_util::tripwire::Module<RenderGlyphFp, TwOom>> =
        core::cell::RefCell::new(rustty_util::tripwire::module());
}

#[cfg(test)]
#[inline]
fn render_glyph_tw_check(point: RenderGlyphFp) -> Result<(), RenderError> {
    RENDER_GLYPH_TW
        .with(|tw| tw.borrow_mut().check(point))
        .map_err(|_| RenderError::OutOfMemory)
}

impl<A: Allocator + Copy> SharedGrid<A> {
    /// The grayscale font atlas.
    //
    // PORT NOTE: the reference `atlas_grayscale` field is PUBLIC — the renderer's
    // `drawFrame` reads `self.font_grid.atlas_grayscale` directly (to check
    // `.modified` and to sync it into the GPU texture). rustty narrowed the field
    // to private, so the renderer reads it through this accessor (the same narrowing
    // precedent as `metal_impl.rs::init_atlas_texture`, whose PORT NOTE says "the
    // Zig public fields are narrowed … the renderer reads them through the accessors").
    pub fn atlas_grayscale(&self) -> &Atlas<A> {
        &self.atlas_grayscale
    }

    /// The color (emoji) font atlas. See [`atlas_grayscale`](Self::atlas_grayscale).
    pub fn atlas_color(&self) -> &Atlas<A> {
        &self.atlas_color
    }

    /// Initialize the grid.
    ///
    /// The resolver must have a collection that supports deferred loading
    /// (collection.load_options != null). This is because we need the load
    /// options data to determine grid metrics and setup our sprite font.
    ///
    /// SharedGrid always configures the sprite font. This struct is expected to be
    /// used with a terminal grid and therefore the sprite font is always
    /// necessary for correct rendering.
    // pub fn init(alloc: Allocator, resolver: CodepointResolver) !SharedGrid {
    //
    // PORT NOTE (N404, UD350): the reference takes the resolver BY VALUE and
    // leaves it to the caller on failure ("The caller is responsible for
    // cleaning up the resolver if init fails", its `init error` test, which
    // `defer resolver.deinit(alloc)`s). A Rust move leaves the caller nothing
    // to clean up, and `Collection` has no `Drop` (its entries own native
    // handles freed only by `deinit`), so every error return here leaked the
    // collection's faces. `init` therefore deinits the resolver it consumed on
    // every error path. That also closes the reference's own leak in
    // `SharedGridSet.ref` (UD350, PORT NOTE there).
    pub fn init(alloc: A, mut resolver: CodepointResolver) -> Result<SharedGrid<A>, InitError> {
        // const tw = init_tw;

        // We need to support loading options since we use the size data
        assert!(resolver.collection.load_options.is_some());

        let atlas_grayscale = match Atlas::init(alloc, 512, Format::Grayscale) {
            Ok(a) => a,
            Err(_) => {
                resolver.deinit();
                return Err(InitError::OutOfMemory);
            }
        };
        // errdefer atlas_grayscale.deinit(alloc); — freed on drop on the error path.
        let atlas_color = match Atlas::init(alloc, 512, Format::Bgra) {
            Ok(a) => a,
            Err(_) => {
                resolver.deinit();
                return Err(InitError::OutOfMemory);
            }
        };
        // errdefer atlas_color.deinit(alloc);

        let mut result: SharedGrid<A> = SharedGrid {
            resolver,
            atlas_grayscale,
            atlas_color,
            lock: RwLock::new(()),
            // .metrics = undefined, // Loaded below
            metrics: UNDEFINED_METRICS,
            codepoints: HashMap::default(),
            glyphs: HashMap::default(),
        };

        match result.init_fill() {
            Ok(()) => Ok(result),
            Err(e) => {
                // The atlases and caches free when `result` drops; the
                // resolver's collection needs its explicit `deinit`.
                result.resolver.deinit();
                Err(e)
            }
        }
    }

    /// The fallible remainder of [`init`](Self::init), after the grid owns
    /// the resolver: its errors return to `init`, which deinits the resolver.
    fn init_fill(&mut self) -> Result<(), InitError> {
        let result = self;
        // We set an initial capacity that can fit a good number of characters.
        // This number was picked empirically based on my own terminal usage.
        // try tw.check(.codepoints_capacity);
        #[cfg(test)]
        init_tw_check(InitFp::CodepointsCapacity)?;
        // try result.codepoints.ensureTotalCapacity(alloc, 128);
        // errdefer result.codepoints.deinit(alloc); — freed on drop of `result`.
        result.codepoints.reserve(128);
        // try tw.check(.glyphs_capacity);
        #[cfg(test)]
        init_tw_check(InitFp::GlyphsCapacity)?;
        // try result.glyphs.ensureTotalCapacity(alloc, 128);
        // errdefer result.glyphs.deinit(alloc);
        result.glyphs.reserve(128);

        // Initialize our metrics.
        // try tw.check(.reload_metrics);
        #[cfg(test)]
        init_tw_check(InitFp::ReloadMetrics)?;
        // try result.reloadMetrics();
        result.reload_metrics()?;

        Ok(())
    }

    /// Deinit. Assumes no concurrent access so no lock is taken.
    // pub fn deinit(self: *SharedGrid, alloc: Allocator) void {
    //     self.codepoints.deinit(alloc);
    //     self.glyphs.deinit(alloc);
    //     self.atlas_grayscale.deinit(alloc);
    //     self.atlas_color.deinit(alloc);
    //     self.resolver.deinit(alloc);
    // }
    //
    // PORT NOTE: the caches clear here; `resolver.deinit` frees the collection's
    // faces. The two `Atlas` grids own their storage (`Vec<u8, A>`) and free it
    // when the `SharedGrid` value itself drops (their `deinit` consumes `self`, so
    // it cannot be called through `&mut self` — RAII covers it identically).
    pub fn deinit(&mut self) {
        self.codepoints.clear();
        self.glyphs.clear();
        self.resolver.deinit();
    }

    // fn reloadMetrics(self: *SharedGrid) !void {
    fn reload_metrics(&mut self) -> Result<(), collection::Error> {
        let collection = &mut self.resolver.collection;
        collection.update_metrics()?;

        self.metrics = self.resolver.collection.metrics.unwrap();

        // Setup our sprite font.
        // self.resolver.sprite = .{ .metrics = self.metrics };
        self.resolver.sprite = Some(rustty_font::sprite::Face {
            metrics: self.metrics,
        });
        Ok(())
    }

    /// Returns the grid cell size.
    ///
    /// This is not thread safe.
    // pub fn cellSize(self: *SharedGrid) renderer.CellSize {
    pub fn cell_size(&self) -> CellSize {
        CellSize {
            width: self.metrics.cell_width,
            height: self.metrics.cell_height,
        }
    }

    /// Get the font index for a given codepoint. This is cached.
    ///
    /// This always forces loading any deferred fonts since we assume that if
    /// you're looking up an index that the caller plans to use the font. By
    /// loading the font in this function we can ensure thread-safety on the
    /// load without complicating future calls.
    // pub fn getIndex(self, alloc, cp, style, p) !?Collection.Index {
    //
    // PORT NOTE: the allocator here (used only by `resolver.getIndex` for
    // codepoint-map/fallback lookups) is independent of the atlas allocator `A`, so
    // it is a separate generic `B` — this lets the shaper (`shaper/run.zig`) call
    // `getIndex` with its own allocator against a `SharedGrid<Global>`.
    pub fn get_index<B: Allocator + Copy>(
        &mut self,
        alloc: B,
        cp: u32,
        style: Style,
        p: Option<Presentation>,
    ) -> Result<Option<collection::Index>, collection::Error> {
        // const key = CodepointKey.from(.{ .style = style, .codepoint = cp, .presentation = p });
        let key = CodepointKey::from(style, cp, p);

        // Fast path: the cache has the value. This is almost always true and
        // only requires a read lock.
        {
            // self.lock.lockShared(); defer self.lock.unlockShared();
            let _read = self.lock.read().unwrap();
            // if (self.codepoints.get(key)) |v| return v;
            if let Some(v) = self.codepoints.get(&key) {
                return Ok(*v);
            }
        }

        // Slow path: we need to search this codepoint
        // self.lock.lock(); defer self.lock.unlock();
        let _write = self.lock.write().unwrap();

        // Try to get it, if it is now in the cache another thread beat us to it.
        // const gop = try self.codepoints.getOrPut(alloc, key);
        // if (gop.found_existing) return gop.value_ptr.*;
        if let Some(v) = self.codepoints.get(&key) {
            return Ok(*v);
        }
        // errdefer self.codepoints.removeByPtr(gop.key_ptr);

        // Load a value and cache it. This even caches negative matches.
        let value = self.resolver.get_index(alloc, cp, style, p);
        // gop.value_ptr.* = value;
        self.codepoints.insert(key, value);

        if let Some(idx) = value {
            'preload: {
                // If the font is a sprite font then we don't need to preload
                // because getFace doesn't work with special fonts.
                if idx.special().is_some() {
                    break 'preload;
                }

                // Load the face in case its deferred. If this fails then we would've
                // failed to load it in the future anyways so we want to undo all
                // the caching we did.
                // _ = try self.resolver.collection.getFace(idx);
                if let Err(e) = self.resolver.collection.get_face(idx) {
                    // errdefer self.codepoints.removeByPtr(gop.key_ptr);
                    self.codepoints.remove(&key);
                    return Err(e);
                }
            }
        }

        Ok(value)
    }

    /// Returns true if the given font index has the codepoint and presentation.
    // pub fn hasCodepoint(self, idx, cp, p) bool {
    pub fn has_codepoint(&self, idx: collection::Index, cp: u32, p: Option<Presentation>) -> bool {
        // self.lock.lockShared(); defer self.lock.unlockShared();
        let _read = self.lock.read().unwrap();
        self.resolver.collection.has_codepoint(
            idx,
            cp,
            // if (p) |v| .{ .explicit = v } else .{ .any = {} }
            match p {
                Some(v) => PresentationMode::Explicit(v),
                None => PresentationMode::Any,
            },
        )
    }

    /// Render a codepoint. This uses the first font index that has the codepoint
    /// and matches the presentation requested. If the codepoint cannot be found
    /// in any font, an null render is returned.
    // pub fn renderCodepoint(self, alloc, cp, style, p, opts) !?Render {
    pub fn render_codepoint(
        &mut self,
        alloc: A,
        cp: u32,
        style: Style,
        p: Option<Presentation>,
        opts: RenderOptions,
    ) -> Result<Option<Render>, RenderError> {
        // Note: we could optimize the below to use way less locking, but
        // at the time of writing this codepath is only called for preedit
        // text which is relatively rare and almost non-existent in multiple
        // surfaces at the same time.

        // Get the font that has the codepoint
        // const index = try self.getIndex(alloc, cp, style, p) orelse return null;
        let index = match self.get_index(alloc, cp, style, p)? {
            Some(i) => i,
            None => return Ok(None),
        };

        // Get the glyph for the font
        let glyph_index: u32 = {
            // self.lock.lockShared(); defer self.lock.unlockShared();
            let _read = self.lock.read().unwrap();
            let face = self.resolver.collection.get_face(index)?;
            // break :glyph_index face.glyphIndex(cp) orelse return null;
            match unsafe { &*face }.glyph_index(cp) {
                Some(gi) => gi,
                None => return Ok(None),
            }
        };

        // Render
        Ok(Some(self.render_glyph(alloc, index, glyph_index, opts)?))
    }

    /// Render a glyph index. This automatically determines the correct texture
    /// atlas to use and caches the result.
    // pub fn renderGlyph(self, alloc, index, glyph_index, opts) !Render {
    pub fn render_glyph(
        &mut self,
        alloc: A,
        index: collection::Index,
        glyph_index: u32,
        opts: RenderOptions,
    ) -> Result<Render, RenderError> {
        // const tw = renderGlyph_tw;

        // const key = GlyphKey.from(.{ .index = index, .glyph = glyph_index, .opts = opts });
        let key = GlyphKey::from(index, glyph_index, &opts);

        // Fast path: the cache has the value. This is almost always true and
        // only requires a read lock.
        {
            // self.lock.lockShared(); defer self.lock.unlockShared();
            let _read = self.lock.read().unwrap();
            // if (self.glyphs.get(key)) |v| return v;
            if let Some(v) = self.glyphs.get(&key) {
                return Ok(*v);
            }
        }

        // Slow path: we need to search this codepoint
        // self.lock.lock(); defer self.lock.unlock();
        let _write = self.lock.write().unwrap();

        // const gop = try self.glyphs.getOrPut(alloc, key);
        // if (gop.found_existing) return gop.value_ptr.*;
        if let Some(v) = self.glyphs.get(&key) {
            return Ok(*v);
        }
        // errdefer self.glyphs.removeByPtr(gop.key_ptr);

        // Get the presentation to determine what atlas to use
        // try tw.check(.get_presentation);
        // (errdefer self.glyphs.removeByPtr(gop.key_ptr) — nothing was inserted yet
        // on this path, so `?` rolls back nothing.)
        #[cfg(test)]
        render_glyph_tw_check(RenderGlyphFp::GetPresentation)?;
        // const p = try self.resolver.getPresentation(index, glyph_index);
        let p = self.resolver.get_presentation(index, glyph_index)?;
        // const atlas: *font.Atlas = switch (p) { .text => &grayscale, .emoji => &color };
        // (We resolve which atlas below by branching; `render_glyph` on the resolver
        // borrows the chosen atlas mutably.)

        let mut render_opts = opts;

        // Always use these constraints for emoji.
        if p == Presentation::Emoji {
            render_opts.constraint = rustty_font::glyph::Constraint {
                // Scale emoji to be as large as possible
                // while preserving their aspect ratio.
                size: rustty_font::glyph::ConstraintSize::Cover,

                // Center the emoji in its cells.
                align_horizontal: rustty_font::glyph::Align::Center,
                align_vertical: rustty_font::glyph::Align::Center,

                // Add a small bit of padding so the emoji
                // doesn't quite touch the edges of the cells.
                pad_left: 0.025,
                pad_right: 0.025,

                // (all other Constraint fields keep their `.none` defaults)
                ..rustty_font::glyph::Constraint::none
            };
        }

        // Render into the atlas
        // const glyph = self.resolver.renderGlyph(alloc, atlas, index, glyph_index, render_opts)
        //     catch |err| switch (err) {
        //         error.AtlasFull => blk: { try atlas.grow(alloc, atlas.size * 2); ... },
        //         else => return err,
        //     };
        let glyph = match render_into(
            &mut self.resolver,
            p,
            &mut self.atlas_grayscale,
            &mut self.atlas_color,
            alloc,
            index,
            glyph_index,
            render_opts,
        ) {
            Ok(g) => g,
            Err(err) => {
                // If the atlas is full, we resize it
                if is_atlas_full(&err) {
                    let atlas: &mut Atlas<A> = match p {
                        Presentation::Text => &mut self.atlas_grayscale,
                        Presentation::Emoji => &mut self.atlas_color,
                    };
                    // try atlas.grow(alloc, atlas.size * 2);
                    let new_size = atlas.size() * 2;
                    atlas
                        .grow(alloc, new_size)
                        .map_err(|_| RenderError::OutOfMemory)?;
                    render_into(
                        &mut self.resolver,
                        p,
                        &mut self.atlas_grayscale,
                        &mut self.atlas_color,
                        alloc,
                        index,
                        glyph_index,
                        render_opts,
                    )?
                } else {
                    return Err(err.into());
                }
            }
        };

        // Cache and return
        // gop.value_ptr.* = .{ .glyph = glyph, .presentation = p };
        let render = Render {
            glyph,
            presentation: p,
        };
        self.glyphs.insert(key, render);

        // return gop.value_ptr.*;
        Ok(render)
    }
}

/// Dispatch `resolver.renderGlyph` to the atlas matching the presentation. This
/// is a small helper so the reference `const atlas: *font.Atlas = switch (p)`
/// selection composes with Rust's borrow rules (only the chosen atlas is
/// borrowed mutably per call).
#[allow(clippy::too_many_arguments)]
fn render_into<A: Allocator + Copy>(
    resolver: &mut CodepointResolver,
    p: Presentation,
    atlas_grayscale: &mut Atlas<A>,
    atlas_color: &mut Atlas<A>,
    alloc: A,
    index: collection::Index,
    glyph_index: u32,
    render_opts: RenderOptions,
) -> Result<Glyph, crate::codepoint_resolver::Error> {
    let atlas: &mut Atlas<A> = match p {
        Presentation::Text => atlas_grayscale,
        Presentation::Emoji => atlas_color,
    };
    resolver.render_glyph(alloc, atlas, index, glyph_index, render_opts)
}

/// Returns true if the resolver error is `error.AtlasFull` (surfaced from the
/// backend face's atlas reserve, or the sprite face's atlas reserve).
fn is_atlas_full(err: &crate::codepoint_resolver::Error) -> bool {
    use crate::codepoint_resolver::Error as E;
    match err {
        E::Face(fe) => matches!(fe, crate::face::FaceError::AtlasFull),
        E::SpriteRender(rustty_font::sprite::face::RenderGlyphError::Reserve(
            rustty_font::atlas::ReserveError::AtlasFull,
        )) => true,
        _ => false,
    }
}

/// `.{ .metrics = self.metrics }` placeholder metrics used before
/// `reloadMetrics` runs (the reference initializes `metrics = undefined`).
const UNDEFINED_METRICS: Metrics = Metrics {
    cell_width: 0,
    cell_height: 0,
    cell_baseline: 0,
    underline_position: 0,
    underline_thickness: 0,
    strikethrough_position: 0,
    strikethrough_thickness: 0,
    overline_position: 0,
    overline_thickness: 0,
    box_thickness: 0,
    cursor_thickness: 0,
    cursor_height: 0,
    icon_height: 0.0,
    icon_height_single: 0.0,
    face_width: 0.0,
    face_height: 0.0,
    face_y: 0.0,
};

// pub const Render = struct { glyph: Glyph, presentation: Presentation };
#[derive(Clone, Copy, Debug)]
pub struct Render {
    pub glyph: Glyph,
    pub presentation: Presentation,
}

// const CodepointKey = packed struct(u64) {
//     codepoint: u32,
//     style: Style,
//     has_presentation: bool,
//     presentation: Presentation,
//     _padding: u27 = 0,
//     ...
// };
//
// PORT NOTE: Rust has no `packed struct(u64)`, so the key IS its backing `u64`
// (Zig packed structs are LSB-first):
//   codepoint: u32          bits  0..32
//   style: Style (u3)       bits 32..35
//   has_presentation: bool  bit  35
//   presentation: u1        bit  36
//   _padding: u27 = 0       bits 37..64
// `#[derive(PartialEq, Eq)]` over the `u64` is the reference's `Context.eql`
// (`@as(u64, @bitCast(a)) == @as(u64, @bitCast(b))`), and `Hash` feeds the
// `u64` to `PackedKeyHasher`, which is the reference's `Context.hash`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(transparent)]
struct CodepointKey(u64);

impl CodepointKey {
    // inline fn from(k: struct {
    //     style: Style,
    //     codepoint: u32,
    //     presentation: ?Presentation,
    // }) CodepointKey {
    #[inline(always)]
    fn from(style: Style, codepoint: u32, presentation: Option<Presentation>) -> Self {
        // .codepoint = k.codepoint,
        let codepoint = codepoint as u64;
        // .style = k.style,
        let style = (style as u64) & 0x7;
        // .has_presentation = k.presentation != null,
        let has_presentation = u64::from(presentation.is_some());
        // .presentation = k.presentation orelse .text,
        let presentation = (presentation.unwrap_or(Presentation::Text) as u64) & 0x1;
        Self(codepoint | (style << 32) | (has_presentation << 35) | (presentation << 36))
    }
}

impl Hash for CodepointKey {
    #[inline(always)]
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_u64(self.0);
    }
}

/// Cache key for rendered glyphs. Packed to 8 bytes so HashMap stores
/// and compares a u64 instead of a full RenderOptions (metrics,
/// nerd-font constraint, etc. are not part of the identity).
// const GlyphKey = packed struct(u64) {
//     glyph: u32,
//     index: Collection.Index,
//     opts: packed struct(u16) {
//         cell_width: u2,
//         thicken: bool,
//         thicken_strength: u8,
//         constraint_width: u2,
//         _padding: u3 = 0,
//     },
//     ...
// };
//
// PORT NOTE: as for `CodepointKey`, the key IS its backing `u64` (LSB-first):
//   glyph: u32                bits  0..32
//   index: Collection.Index   bits 32..48 (its u16 backing, `Index.int()`)
//   opts: packed(u16)         bits 48..64
//     cell_width: u2             bits 48..50
//     thicken: bool              bit  50
//     thicken_strength: u8       bits 51..59
//     constraint_width: u2       bits 59..61
//     _padding: u3 = 0           bits 61..64
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(transparent)]
struct GlyphKey(u64);

impl GlyphKey {
    // inline fn from(k: struct {
    //     index: Collection.Index,
    //     glyph: u32,
    //     opts: RenderOptions,
    // }) GlyphKey {
    //
    // PORT NOTE: `opts` is borrowed rather than copied; only four of its
    // fields are read.
    #[inline(always)]
    fn from(index: collection::Index, glyph: u32, opts: &RenderOptions) -> Self {
        // .glyph = k.glyph,
        let glyph = glyph as u64;
        // .index = k.index,
        let index = index.int() as u64;
        // .cell_width = k.opts.cell_width orelse 0,
        let cell_width = (opts.cell_width.unwrap_or(0) as u64) & 0x3;
        // .thicken = k.opts.thicken,
        let thicken = u64::from(opts.thicken);
        // .thicken_strength = k.opts.thicken_strength,
        let thicken_strength = (opts.thicken_strength as u64) & 0xFF;
        // .constraint_width = k.opts.constraint_width,
        let constraint_width = (opts.constraint_width as u64) & 0x3;
        let opts = cell_width | (thicken << 2) | (thicken_strength << 3) | (constraint_width << 11);
        Self(glyph | (index << 32) | (opts << 48))
    }
}

impl Hash for GlyphKey {
    #[inline(always)]
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_u64(self.0);
    }
}

// const Context = struct {
//     pub fn hash(_: Context, key: CodepointKey) u64 {
//         const x: u64 = @bitCast(key);
//         return x ^ (x >> 32);
//     }
//
//     pub fn eql(_: Context, a: CodepointKey, b: CodepointKey) bool {
//         return @as(u64, @bitCast(a)) == @as(u64, @bitCast(b));
//     }
// };
//
// PORT NOTE: `CodepointKey.Context` and `GlyphKey.Context` have the same
// `hash`, so both caches share this one hasher in place of std's SipHash.
// Each key's `Hash` impl makes exactly one `write_u64` call with its packed
// `u64`, and `finish` returns `x ^ (x >> 32)` of it. The reference's
// `max_load_percentage` of 80 has no std `HashMap` counterpart.
#[derive(Default)]
struct PackedKeyHasher(u64);

impl Hasher for PackedKeyHasher {
    #[inline(always)]
    fn write_u64(&mut self, x: u64) {
        self.0 = x ^ (x >> 32);
    }

    fn write(&mut self, _bytes: &[u8]) {
        unreachable!("PackedKeyHasher only hashes a packed u64 key via write_u64");
    }

    #[inline(always)]
    fn finish(&self) -> u64 {
        self.0
    }
}

type PackedKeyState = BuildHasherDefault<PackedKeyHasher>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collection::{AddOptions, Collection, LoadOptions, SizeAdjustment};
    use crate::face::{DesiredSize, Face, Options};
    use crate::library::Library;
    use allocator_api2::alloc::Global;
    use rustty_util::tripwire::ResetMode;

    // const TestMode = enum { normal };
    #[derive(Clone, Copy)]
    enum TestMode {
        Normal,
    }

    fn opts(size: DesiredSize) -> Options {
        Options {
            size,
            freetype_load_flags: crate::face::FREETYPE_LOAD_FLAGS_DEFAULT,
        }
    }

    // fn testGrid(mode: TestMode, alloc: Allocator, lib: Library) !SharedGrid {
    fn test_grid(
        mode: TestMode,
        alloc: Global,
        lib: Library,
    ) -> Result<SharedGrid<Global>, InitError> {
        let test_font = rustty_font::embedded::REGULAR;

        let mut c = Collection::init();
        // c.load_options = .{ .library = lib };
        c.load_options = Some(LoadOptions::new(lib));

        match mode {
            TestMode::Normal => {
                c.add(
                    Face::init(
                        lib,
                        test_font,
                        opts(DesiredSize {
                            points: 12.0,
                            xdpi: 96,
                            ydpi: 96,
                        }),
                    )
                    .unwrap(),
                    AddOptions {
                        style: Style::Regular,
                        fallback: false,
                        size_adjustment: SizeAdjustment::None,
                    },
                )
                .unwrap();
            }
        }

        // var r: CodepointResolver = .{ .collection = c };
        let r = CodepointResolver {
            collection: c,
            styles: crate::codepoint_resolver::StyleStatus::default(),
            discover: None,
            codepoint_map: None,
            descriptor_cache: crate::codepoint_resolver::DescriptorCache::default(),
            sprite: None,
        };
        // errdefer r.deinit(alloc); — on the `init` error path, `r` was moved into
        // `init`, which drops it (freeing its collection). See `init`'s PORT NOTE.

        SharedGrid::init(alloc, r)
    }

    // test getIndex
    #[test]
    fn get_index() {
        let alloc = Global;
        // const testEmoji = @import("test.zig").fontEmoji;

        let mut lib = Library::init().unwrap();

        let mut grid = test_grid(TestMode::Normal, alloc, lib).unwrap();

        use crate::codepoint_resolver::TEST_GET_INDEX_CALLS;
        let resolver_calls = || TEST_GET_INDEX_CALLS.with(|c| c.get());
        TEST_GET_INDEX_CALLS.with(|c| c.set(0));

        // Visible ASCII.
        for i in 32u32..127 {
            let idx = grid
                .get_index(alloc, i, Style::Regular, None)
                .unwrap()
                .unwrap();
            assert_eq!(Style::Regular, idx.style());
            assert_eq!(0u16, idx.idx());
            assert!(grid.has_codepoint(idx, i, None));
        }

        // Do it again without a resolver set to ensure we only hit the cache
        // const old_resolver = grid.resolver; grid.resolver = undefined; ...
        //
        // PORT NOTE (parity S86-F3): the reference swaps in an `undefined`
        // resolver, so ANY consultation of it in the second pass is UB and the
        // test dies. Rust has no `undefined` and we cannot move `resolver` out
        // of `grid` and keep `grid` usable — so the witness is a test-only
        // counter bumped INSIDE `CodepointResolver::get_index`, i.e. inside the
        // code the cache is supposed to keep us out of. Asserting only that the
        // second pass returns the same values (which is all this test used to
        // do) proves nothing: a cache that was never consulted returns the same
        // values too.
        //
        // The first pass must have REACHED the resolver — otherwise the
        // second pass's zero is vacuous.
        let first_pass_calls = resolver_calls();
        assert!(
            first_pass_calls > 0,
            "the first pass must reach the resolver, else the second pass's \
             count proves nothing"
        );
        TEST_GET_INDEX_CALLS.with(|c| c.set(0));
        for i in 32u32..127 {
            let idx = grid
                .get_index(alloc, i, Style::Regular, None)
                .unwrap()
                .unwrap();
            assert_eq!(Style::Regular, idx.style());
            assert_eq!(0u16, idx.idx());
        }
        assert_eq!(
            0,
            resolver_calls(),
            "the second pass must be served entirely from the cache"
        );

        grid.deinit();
        lib.deinit();
    }

    // test "renderGlyph error after cache insert rolls back cache entry"
    #[test]
    // WS18 release lane (CORRECTED, parity S83-F3 / S86-F4): this test injects a
    // failure through a `rustty_util::tripwire` fail point.
    // `tripwire::Module::enabled()` is NOT `cfg!(debug_assertions)` — it is
    // `cfg!(any(test, debug_assertions, feature = "tripwire-test"))`, and this
    // crate now turns on `rustty-util/tripwire-test` as a DEV-dependency
    // feature, so the fail point trips in the release test lane as well. The
    // reference runs its `FailingAllocator` equivalent at every optimize mode;
    // the `#[ignore]` that used to sit here made the release lane green by
    // skipping.
    fn render_glyph_error_after_cache_insert_rolls_back_cache_entry() {
        // This test verifies that when renderGlyph fails after inserting a cache
        // entry (via getOrPut), the errdefer properly removes the entry, preventing
        // corrupted/uninitialized data from remaining in the cache.

        let alloc = Global;

        let mut lib = Library::init().unwrap();

        let mut grid = test_grid(TestMode::Normal, alloc, lib).unwrap();

        // Get the font index for 'A'
        let idx = grid
            .get_index(alloc, u32::from('A'), Style::Regular, None)
            .unwrap()
            .unwrap();

        // Get the glyph index for 'A'
        let glyph_index: u32 = {
            let _read = grid.lock.read().unwrap();
            let face = grid.resolver.collection.get_face(idx).unwrap();
            unsafe { &*face }.glyph_index(u32::from('A')).unwrap()
        };

        let render_opts: RenderOptions = RenderOptions {
            grid_metrics: grid.metrics,
            ..Default::default()
        };
        // const key = GlyphKey.from(.{ .index = idx, .glyph = glyph_index, .opts = render_opts });
        let key = GlyphKey::from(idx, glyph_index, &render_opts);

        // Verify the cache is empty for this glyph
        assert!(!grid.glyphs.contains_key(&key)); // grid.glyphs.get(key) == null

        // Set up tripwire to fail after cache insert.
        // We use OutOfMemory as it's a valid error in the renderGlyph error set.
        // const tw = renderGlyph_tw; defer tw.end(.reset) catch {};
        // tw.errorAlways(.get_presentation, error.OutOfMemory);
        RENDER_GLYPH_TW.with(|tw| {
            tw.borrow_mut()
                .errorAlways(RenderGlyphFp::GetPresentation, TwOom::OutOfMemory)
        });

        // This should fail due to the tripwire
        assert!(matches!(
            grid.render_glyph(alloc, idx, glyph_index, render_opts),
            Err(RenderError::OutOfMemory)
        ));

        // defer tw.end(.reset) catch {};
        let _ = RENDER_GLYPH_TW.with(|tw| tw.borrow_mut().end(ResetMode::Reset));

        // The errdefer should have removed the cache entry, leaving the cache clean.
        // Without the errdefer fix, this would contain garbage/uninitialized data.
        assert!(!grid.glyphs.contains_key(&key)); // grid.glyphs.get(key) == null

        grid.deinit();
        lib.deinit();
    }

    /// LANE GUARD (parity S83-F3 / S86-F4), the tripwire analogue of
    /// `RUSTTY_REQUIRE_GL` / `RUSTTY_REQUIRE_METAL`: a test lane that is
    /// supposed to exercise the fail-point tests must not be able to go green
    /// with the fail points compiled out.
    ///
    /// An env-var switch of the GL/Metal shape does not fit this case — those
    /// convert a RUNTIME capability probe into a failure, whereas
    /// `tripwire::Module::enabled()` is a compile-time `cfg!`, and a skip here
    /// is spelled `#[ignore]`/`#[cfg]`, which no runtime check inside a test
    /// body can observe because the body never runs. A lane-level assertion
    /// can: it fails loudly in any build where the tripwires are disabled,
    /// which is precisely the build in which every fail-point test below is
    /// vacuous.
    #[test]
    fn tripwire_fail_points_are_armed_in_this_test_build() {
        assert!(
            rustty_util::tripwire::Module::<InitFp, TwOom>::enabled(),
            "tripwire fail points are compiled OUT of this build, so every \
             fail-point test in this crate is vacuous. Enable \
             `rustty-util/tripwire-test` as a dev-dependency feature."
        );
    }

    // test "init error"
    #[test]
    // WS18 release lane (CORRECTED, parity S83-F3 / S86-F4): this test injects a
    // failure through a `rustty_util::tripwire` fail point.
    // `tripwire::Module::enabled()` is NOT `cfg!(debug_assertions)` — it is
    // `cfg!(any(test, debug_assertions, feature = "tripwire-test"))`, and this
    // crate now turns on `rustty-util/tripwire-test` as a DEV-dependency
    // feature, so the fail point trips in the release test lane as well. The
    // reference runs its `FailingAllocator` equivalent at every optimize mode;
    // the `#[ignore]` that used to sit here made the release lane green by
    // skipping.
    fn init_error() {
        // Test every failure point in `init` and ensure that we don't
        // leak memory (testing.allocator verifies) since we're exiting early.
        //
        // BUG: Currently this test will fail because init() is missing errdefer
        // cleanup for codepoints and glyphs when late operations fail
        // (ensureTotalCapacity, reloadMetrics).
        let alloc = Global;

        // for (std.meta.tags(init_tw.FailPoint)) |tag| {
        for tag in [
            InitFp::CodepointsCapacity,
            InitFp::GlyphsCapacity,
            InitFp::ReloadMetrics,
        ] {
            // const tw = init_tw; defer tw.end(.reset) catch unreachable;
            // tw.errorAlways(tag, error.OutOfMemory);
            INIT_TW.with(|tw| tw.borrow_mut().errorAlways(tag, TwOom::OutOfMemory));

            // Create a resolver for testing - we need to set up a minimal one.
            // The caller is responsible for cleaning up the resolver if init fails.
            let mut lib = Library::init().unwrap();

            let mut c = Collection::init();
            c.load_options = Some(LoadOptions::new(lib));
            c.add(
                Face::init(
                    lib,
                    rustty_font::embedded::REGULAR,
                    opts(DesiredSize {
                        points: 12.0,
                        xdpi: 96,
                        ydpi: 96,
                    }),
                )
                .unwrap(),
                AddOptions {
                    style: Style::Regular,
                    fallback: false,
                    size_adjustment: SizeAdjustment::None,
                },
            )
            .unwrap();

            // var resolver: CodepointResolver = .{ .collection = c };
            let resolver = CodepointResolver {
                collection: c,
                styles: crate::codepoint_resolver::StyleStatus::default(),
                discover: None,
                codepoint_map: None,
                descriptor_cache: crate::codepoint_resolver::DescriptorCache::default(),
                sprite: None,
            };
            // defer resolver.deinit(alloc); // Caller cleans up on init failure
            //
            // PORT NOTE (N404): `resolver` is moved into `init`, so the caller
            // cannot clean it up; `init` deinits it on its error path instead,
            // one teardown either way. A drop alone does NOT free the faces
            // (`Collection` has no `Drop`): this test used to say it did, and
            // memcheck found the collection's face leaked on every iteration.
            // So the teardown is asserted, not assumed: the one loaded face is
            // deinited by the failing `init`.
            let deinits_before = crate::face::test_probe::face_deinits();

            // try testing.expectError(error.OutOfMemory, init(alloc, resolver));
            assert!(matches!(
                SharedGrid::init(alloc, resolver),
                Err(InitError::OutOfMemory)
            ));
            assert_eq!(
                crate::face::test_probe::face_deinits(),
                deinits_before + 1,
                "{tag:?}: the failed init deinits the resolver's one face"
            );

            // defer tw.end(.reset) catch unreachable;
            //
            // `.unwrap()`, NOT `let _ =` (parity S86-F4): the reference here is
            // `catch unreachable`, and discarding the `UntrippedError` is what
            // lets a fail point that never trips pass unnoticed — which is
            // exactly the failure mode the release lane had while this test was
            // `#[ignore]`d there.
            INIT_TW
                .with(|tw| tw.borrow_mut().end(ResetMode::Reset))
                .unwrap();

            lib.deinit();
        }
    }
}
