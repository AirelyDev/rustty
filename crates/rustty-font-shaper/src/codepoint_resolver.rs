//! CodepointResolver maps a codepoint to a font. It is more dynamic
//! than "Collection" since it supports mapping codepoint ranges to
//! specific fonts, searching for fallback fonts, and more.
//!
//! To initialize the codepoint resolver, manually initialize using
//! Zig initialization syntax: .{}-style. Set the fields you want set,
//! and begin using the resolver.
//!
//! Deinit must still be called on the resolver to free any memory
//! allocated during use. All functions that take allocators should use
//! the same allocator.
// const CodepointResolver = @This();

// const std = @import("std");
// const Allocator = std.mem.Allocator;
// const uucode = @import("uucode");
// const font = @import("main.zig");
// const Atlas = font.Atlas;
// const CodepointMap = font.CodepointMap;
// const Collection = font.Collection;
// const Discover = font.Discover;
// const DiscoveryDescriptor = font.discovery.Descriptor;
// const Face = font.Face;
// const Glyph = font.Glyph;
// const Library = font.Library;
// const Presentation = font.Presentation;
// const RenderOptions = font.Glyph.RenderOptions;
// const SpriteFace = font.SpriteFace;
// const Style = font.Style;

use crate::collection::{self, Collection, Entry, PresentationMode};
use crate::discovery::Descriptor as DiscoveryDescriptor;
use crate::{uucode, Discover, Presentation, Style};
use allocator_api2::alloc::Allocator;
use rustty_font::atlas::Atlas;
use rustty_font::codepoint_map::CodepointMap;
use rustty_font::glyph::{Glyph, RenderOptions};
use rustty_font::sprite::Face as SpriteFace;
use std::collections::HashMap;

// const log = std.log.scoped(.font_codepoint_resolver);

/// CodepointResolver: see the module doc comment.
// PORT NOTE: the reference is a file-struct initialized `.{}`-style with the
// fields below (most defaulted). rustty exposes them as public fields; construct
// with `CodepointResolver { collection, ..Default::default() }`-style.
pub struct CodepointResolver {
    /// The underlying collection of fonts. This will be modified as
    /// new fonts are found via the resolver. The resolver takes ownership
    /// of the collection and will deinit it when it is deinitialized.
    pub collection: Collection,

    /// The set of statuses and whether they're enabled or not. This defaults
    /// to true. This can be changed at runtime with no ill effect.
    // styles: StyleStatus = .initFill(true),
    pub styles: StyleStatus,

    /// If discovery is available, we'll look up fonts where we can't find
    /// the codepoint. This can be set after initialization.
    // discover: ?*Discover = null,
    //
    // PORT NOTE: `?*Discover` — a nullable pointer to the (externally owned)
    // discoverer. Modeled as `Option<*mut Discover>` (a raw pointer, matching the
    // reference; the resolver does not own it).
    pub discover: Option<*mut Discover>,

    /// A map of codepoints to font requests for codepoint-level overrides.
    /// The memory associated with the map is owned by the caller and is not
    /// modified or freed by Group.
    // codepoint_map: ?CodepointMap = null,
    pub codepoint_map: Option<CodepointMap>,

    /// The descriptor cache is used to cache the descriptor to font face
    /// mapping for codepoint maps.
    // descriptor_cache: DescriptorCache = .{},
    pub descriptor_cache: DescriptorCache,

    /// Set this to a non-null value to enable sprite glyph drawing. If this
    /// isn't enabled we'll just fall through to trying to use regular fonts
    /// to render sprite glyphs. But more than likely, if this isn't set then
    /// terminal rendering will look wrong.
    // sprite: ?SpriteFace = null,
    pub sprite: Option<SpriteFace>,
}

/// The error set for `getPresentation`/`renderGlyph` (backend-face errors +
/// sprite render errors + collection load errors, consolidated).
#[derive(Debug)]
pub enum Error {
    Collection(collection::Error),
    Face(crate::face::FaceError),
    SpriteRender(rustty_font::sprite::face::RenderGlyphError),
}

impl From<collection::Error> for Error {
    fn from(e: collection::Error) -> Self {
        Error::Collection(e)
    }
}
impl From<crate::face::FaceError> for Error {
    fn from(e: crate::face::FaceError) -> Self {
        Error::Face(e)
    }
}
impl From<rustty_font::sprite::face::RenderGlyphError> for Error {
    fn from(e: rustty_font::sprite::face::RenderGlyphError) -> Self {
        Error::SpriteRender(e)
    }
}

impl CodepointResolver {
    // pub fn deinit(self: *CodepointResolver, alloc: Allocator) void {
    pub fn deinit(&mut self) {
        self.collection.deinit();
        self.descriptor_cache.deinit();
    }

    /// Looks up the font that should be used for a specific codepoint.
    /// The font index is valid as long as font faces aren't removed. This
    /// isn't cached; it is expected that downstream users handle caching if
    /// that is important.
    ///
    /// Optionally, a presentation format can be specified. This presentation
    /// format will be preferred but if it can't be found in this format,
    /// any format will be accepted. If presentation is null, the UCD
    /// (Unicode Character Database) will be used to determine the default
    /// presentation for the codepoint.
    /// a code point.
    ///
    /// An allocator is required because certain functionality (codepoint
    /// mapping, fallback fonts, etc.) may require memory allocation. Curiously,
    /// this function cannot error! If an error occurs for any reason, including
    /// memory allocation, the associated functionality is ignored and the
    /// resolver attempts to use a different method to satisfy the codepoint.
    /// This behavior is intentional to make the resolver apply best-effort
    /// logic to satisfy the codepoint since its better to render something
    /// than nothing.
    ///
    /// This logic is relatively complex so the exact algorithm is documented
    /// here. If this gets out of sync with the code, ask questions.
    ///
    ///   1. If a font style is requested that is disabled (i.e. bold),
    ///      we start over with the regular font style. The regular font style
    ///      cannot be disabled, but it can be replaced with a stylized font
    ///      face.
    ///
    ///   2. If there is a codepoint override for the codepoint, we satisfy
    ///      that requirement if we can, no matter what style or presentation.
    ///
    ///   3. If this is a sprite codepoint (such as an underline), then the
    ///      sprite font always is the result.
    ///
    ///   4. If the exact style and presentation request can be satisfied by
    ///      one of our loaded fonts, we return that value. We search loaded
    ///      fonts in the order they're added to the group, so the caller must
    ///      set the priority order.
    ///
    ///   5. If the style isn't regular, we restart this process at this point
    ///      but with the regular style. This lets us fall back to regular with
    ///      our loaded fonts before trying a fallback. We'd rather show a regular
    ///      version of a codepoint from a loaded font than find a new font in
    ///      the correct style because styles in other fonts often change
    ///      metrics like glyph widths.
    ///
    ///   6. If the style is regular, and font discovery is enabled, we look
    ///      for a fallback font to satisfy our request.
    ///
    ///   7. Finally, as a last resort, we fall back to restarting this whole
    ///      process with a regular font face satisfying ANY presentation for
    ///      the codepoint. If this fails, we return null.
    ///
    // pub fn getIndex(self, alloc, cp, style, p) ?Collection.Index {
    pub fn get_index<A: Allocator + Copy>(
        &mut self,
        alloc: A,
        cp: u32,
        style: Style,
        p: Option<Presentation>,
    ) -> Option<collection::Index> {
        // If we've disabled a font style, then fall back to regular.
        if style != Style::Regular && !self.styles.get(style) {
            return self.get_index(alloc, cp, Style::Regular, p);
        }

        // Codepoint overrides.
        match self.get_index_codepoint_override(alloc, cp) {
            Ok(idx_) => {
                if let Some(idx) = idx_ {
                    return Some(idx);
                }
            }
            Err(_err) => {
                // log.warn("codepoint override failed codepoint={} err={}", .{ cp, err });
            }
        }

        // If we have sprite drawing enabled, check if our sprite face can
        // handle this.
        if let Some(sprite) = self.sprite {
            if sprite.has_codepoint(cp, p.map(to_sprite_presentation)) {
                return Some(collection::Index::init_special(
                    collection::index::Special::Sprite,
                ));
            }
        }

        // Build our presentation mode. If we don't have an explicit presentation
        // given then we use the UCD (Unicode Character Database) to determine
        // the default presentation. Note there is some inefficiency here because
        // we'll do this multiple times if we recurse, but this is a cached function
        // call higher up (GroupCache) so this should be rare.
        // const p_mode: Collection.PresentationMode = if (p) |v| .{ .explicit = v } else .{
        //     .default = if (uucode.get(.is_emoji_presentation, @intCast(cp))) .emoji else .text,
        // };
        let p_mode: PresentationMode = if let Some(v) = p {
            PresentationMode::Explicit(v)
        } else {
            PresentationMode::Default(if uucode::is_emoji_presentation(cp) {
                Presentation::Emoji
            } else {
                Presentation::Text
            })
        };

        // If we can find the exact value, then return that.
        if let Some(value) = self.collection.get_index(cp, style, p_mode) {
            return Some(value);
        }

        // If we're not a regular font style, try looking for a regular font
        // that will satisfy this request. Blindly looking for unmatched styled
        // fonts to satisfy one codepoint results in some ugly rendering.
        if style != Style::Regular {
            if let Some(value) = self.get_index(alloc, cp, Style::Regular, p) {
                return Some(value);
            }
        }

        // If we are regular, try looking for a fallback using discovery.
        // if (style == .regular and font.Discover != void) { ... }
        //
        // PORT NOTE: `font.Discover != void` is comptime-true on the macOS/Linux
        // parity backends (CoreText/Fontconfig), so this fallback block is active;
        // the `== void` (freetype/web_canvas) case elides it. In the ported tests
        // `self.discover` is always null, so the `if (self.discover)` guard
        // short-circuits before any real discovery is performed.
        if style == Style::Regular {
            // log.debug("searching for a fallback font for cp={X}", .{cp});
            'discover: {
                if let Some(disco) = self.discover {
                    // SAFETY: `disco` is an externally-owned discoverer pointer.
                    let disco = unsafe { &mut *disco };
                    let load_opts = match self.collection.load_options {
                        Some(o) => o,
                        None => break 'discover,
                    };
                    let mut disco_it = match disco.discover_fallback(
                        &mut self.collection as *mut Collection,
                        DiscoveryDescriptor {
                            codepoint: cp,
                            size: load_opts.size.points,
                            bold: style == Style::Bold || style == Style::BoldItalic,
                            italic: style == Style::Italic || style == Style::BoldItalic,
                            monospace: false,
                            ..DiscoveryDescriptor::default()
                        },
                    ) {
                        Ok(it) => it,
                        Err(_) => break 'discover,
                    };
                    // defer disco_it.deinit();

                    // PORT NOTE (WS20): the faithful shape of the reference's
                    // `while (true) { const deferred_face = (disco_it.next() catch
                    // ...) orelse break; ... }` — TWO distinct exits (the caught
                    // error and the exhausted iterator) that a `while let` would
                    // fold into one, losing the error arm. clippy's `while_let_loop`
                    // only fires on the non-CoreText discovery backends (the macOS
                    // build does not trip it), so the allow is scoped to this loop.
                    #[allow(clippy::while_let_loop)]
                    loop {
                        let deferred_face = match disco_it.next() {
                            Ok(v) => v,
                            Err(_err) => {
                                // log.warn("fallback search failed with error err={}", .{err});
                                break;
                            }
                        };
                        let deferred_face = match deferred_face {
                            Some(d) => d,
                            None => break,
                        };

                        // Discovery is supposed to only return faces that have our
                        // codepoint but we can't search presentation in discovery so
                        // we have to check it here.
                        let face = Entry {
                            face: collection::AnyFace::Deferred(deferred_face),
                            fallback: true,
                            // (scale_factor default `.{ .scale = 1.0 }`)
                            scale_factor: collection::ScaleFactor::Scale(1.0),
                        };
                        if !face.has_codepoint(cp, p_mode) {
                            // Extract the deferred face back out to deinit it.
                            let mut deferred_face = match face.face {
                                collection::AnyFace::Deferred(d) => d,
                                _ => unreachable!(),
                            };
                            deferred_face.deinit();
                            continue;
                        }

                        // Extract the deferred face back out of the temporary entry.
                        let deferred_face = match face.face {
                            collection::AnyFace::Deferred(d) => d,
                            _ => unreachable!(),
                        };

                        let mut buf = [0u8; 256];
                        // log.info("found codepoint 0x{X} in fallback face={s}", .{ cp, deferred_face.name(&buf) catch "<error>" });
                        let _ = deferred_face.name(&mut buf);
                        return match self.collection.add_deferred(
                            deferred_face,
                            collection::AddOptions {
                                style,
                                fallback: true,
                                size_adjustment: crate::DEFAULT_FALLBACK_ADJUSTMENT,
                            },
                        ) {
                            Ok(idx) => Some(idx),
                            Err(_) => {
                                // The deferred face was moved into `add_deferred`
                                // and dropped there on error (releasing its CTFont),
                                // reproducing the reference's `deferred_face.deinit()`.
                                break 'discover;
                            }
                        };
                    }

                    // log.debug("no fallback face found for cp={X}", .{cp});
                }
            }
        }

        // If this is regular with any matching presentation, then we are done
        // there is nothing more we can do. Otherwise we fall through and do
        // an any presentation search.
        // if (style == .regular and p_mode == .any) return null;
        if style == Style::Regular && p_mode == PresentationMode::Any {
            return None;
        }

        // For non-regular fonts, we fall back to regular with any presentation
        self.collection
            .get_index(cp, Style::Regular, PresentationMode::Any)
    }

    /// Checks if the codepoint is in the map of codepoint overrides,
    /// finds the override font, and returns it.
    // fn getIndexCodepointOverride(self, alloc, cp) !?Collection.Index {
    fn get_index_codepoint_override<A: Allocator + Copy>(
        &mut self,
        alloc: A,
        cp: u32,
    ) -> Result<Option<collection::Index>, OverrideError> {
        // If discovery is disabled then we can't do codepoint overrides
        // since the override is based on discovery to find the font.
        // if (comptime font.Discover == void) return null;
        //
        // PORT NOTE: `font.Discover != void` on the parity backends, so this early
        // return is elided.

        // Get our codepoint map. If we have no map set then we have no
        // codepoint overrides and we're done.
        let map = match &self.codepoint_map {
            Some(m) => m,
            None => return Ok(None),
        };

        // If we have a codepoint too large or isn't in the map, then we
        // don't have an override. The map returns a descriptor that can be
        // used with font discovery to search for a matching font.
        // const cp_u21 = std.math.cast(u21, cp) orelse return null;
        if cp > 0x1F_FFFF {
            return Ok(None);
        }
        let cp_u21 = cp;
        // const desc = map.get(cp_u21) orelse return null;
        let desc: DiscoveryDescriptor = match map.get(cp_u21) {
            Some(d) => d.into(),
            None => return Ok(None),
        };

        // Fast path: the descriptor is already loaded. This means that we
        // already did the search before and we have an exact font for this
        // codepoint.
        // const idx_: ?Collection.Index = self.descriptor_cache.get(desc) orelse idx: { ... };
        let idx_: Option<collection::Index> = match self.descriptor_cache.get(&desc) {
            Some(cached) => cached,
            None => {
                // Slow path: we have to find this descriptor and load the font
                let discover = match self.discover {
                    Some(d) => d,
                    None => return Ok(None),
                };
                // SAFETY: externally-owned discoverer pointer.
                let discover = unsafe { &mut *discover };
                let mut disco_it = discover
                    .discover(desc.clone())
                    .map_err(|_| OverrideError::Discover)?;
                // defer disco_it.deinit();

                let face = match disco_it.next().map_err(|_| OverrideError::Discover)? {
                    Some(f) => f,
                    None => {
                        // log.warn("font lookup for codepoint map failed codepoint={} err=FontNotFound", .{cp});
                        // Add null to the cache so we don't do a lookup again later.
                        self.descriptor_cache.put(desc, None);
                        disco_it.deinit();
                        return Ok(None);
                    }
                };

                // Add the font to our list of fonts so we can get an index for it,
                // and ensure the index is stored in the descriptor cache for next time.
                let idx = self
                    .collection
                    .add_deferred(
                        face,
                        collection::AddOptions {
                            style: Style::Regular,
                            fallback: false,
                            size_adjustment: crate::DEFAULT_FALLBACK_ADJUSTMENT,
                        },
                    )
                    .map_err(|_| OverrideError::Add)?;
                self.descriptor_cache.put(desc, Some(idx));

                disco_it.deinit();
                Some(idx)
            }
        };

        // The descriptor cache will populate null if the descriptor is not found
        // to avoid expensive discoveries later, so if it is null then we already
        // searched and found nothing.
        let idx = match idx_ {
            Some(i) => i,
            None => return Ok(None),
        };

        // We need to verify that this index has the codepoint we want.
        if self
            .collection
            .has_codepoint(idx, cp, PresentationMode::Any)
        {
            // log.debug("codepoint override based on config codepoint={} family={s}", .{ cp, desc.family orelse "" });
            let _ = alloc;
            return Ok(Some(idx));
        }

        Ok(None)
    }

    /// Returns the presentation for a specific font index. This is useful for
    /// determining what atlas is needed.
    // pub fn getPresentation(self, index, glyph_index) !Presentation {
    pub fn get_presentation(
        &mut self,
        index: collection::Index,
        glyph_index: u32,
    ) -> Result<Presentation, Error> {
        // if (index.special()) |sp| return switch (sp) { .sprite => .text };
        if let Some(sp) = index.special() {
            return Ok(match sp {
                collection::index::Special::Sprite => Presentation::Text,
            });
        }

        let face = self.collection.get_face(index)?;
        // return if (face.isColorGlyph(glyph_index)) .emoji else .text;
        Ok(if unsafe { &*face }.is_color_glyph(glyph_index) {
            Presentation::Emoji
        } else {
            Presentation::Text
        })
    }

    /// Render a glyph by glyph index into the given font atlas and return
    /// metadata about it.
    ///
    /// This performs no caching, it is up to the caller to cache calls to this
    /// if they want. This will also not resize the atlas if it is full.
    ///
    /// IMPORTANT: this renders by /glyph index/ and not by /codepoint/. The caller
    /// is expected to translate codepoints to glyph indexes in some way. The most
    /// trivial way to do this is to get the Face and call glyphIndex. If you're
    /// doing text shaping, the text shaping library (i.e. HarfBuzz) will automatically
    /// determine glyph indexes for a text run.
    // pub fn renderGlyph(self, alloc, atlas, index, glyph_index, opts) !Glyph {
    pub fn render_glyph<A: Allocator + Copy>(
        &mut self,
        alloc: A,
        atlas: &mut Atlas<A>,
        index: collection::Index,
        glyph_index: u32,
        opts: RenderOptions,
    ) -> Result<Glyph, Error> {
        // Special-case fonts are rendered directly.
        // if (index.special()) |sp| switch (sp) { .sprite => return try self.sprite.?.renderGlyph(...) };
        if let Some(sp) = index.special() {
            match sp {
                collection::index::Special::Sprite => {
                    return Ok(self.sprite.unwrap().render_glyph(
                        alloc,
                        atlas,
                        glyph_index,
                        opts,
                    )?);
                }
            }
        }

        let face = self.collection.get_face(index)?;
        let glyph = unsafe { &*face }.render_glyph(alloc, atlas, glyph_index, opts)?;
        // log.warn("GLYPH={}", .{glyph});
        Ok(glyph)
    }
}

/// The error set for `getIndexCodepointOverride` (`!?Collection.Index`).
#[derive(Debug)]
enum OverrideError {
    Discover,
    Add,
}

/// Packed array of booleans to indicate if a style is enabled or not.
// pub const StyleStatus = std.EnumArray(Style, bool);
//
// PORT NOTE: `EnumArray(Style, bool)` is a fixed 4-element `bool` array indexed by
// `Style`; `.initFill(true)` sets all enabled.
#[derive(Clone, Copy, Debug)]
pub struct StyleStatus([bool; 4]);

impl StyleStatus {
    /// `.initFill(true)` — every style enabled.
    pub fn init_fill(v: bool) -> StyleStatus {
        StyleStatus([v; 4])
    }

    /// `self.styles.get(style)`.
    pub fn get(&self, style: Style) -> bool {
        self.0[style as usize]
    }

    /// `self.styles.set(style, v)`.
    pub fn set(&mut self, style: Style, v: bool) {
        self.0[style as usize] = v;
    }
}

impl Default for StyleStatus {
    fn default() -> Self {
        StyleStatus::init_fill(true)
    }
}

/// Map of descriptors to faces. This is used with manual codepoint maps
/// to ensure that we don't load the same font multiple times.
///
/// Note that the current implementation will load the same font multiple
/// times if the font used for a codepoint map is identical to a font used
/// for a regular style. That's just an inefficient choice made now because
/// the implementation is simpler and codepoint maps matching a regular
/// font is a rare case.
// const DescriptorCache = std.HashMapUnmanaged(
//     DiscoveryDescriptor,
//     ?Collection.Index,
//     struct {
//         const KeyType = DiscoveryDescriptor;
//         pub fn hash(ctx: @This(), k: KeyType) u64 { _ = ctx; return k.hashcode(); }
//         pub fn eql(ctx: @This(), a: KeyType, b: KeyType) bool {
//             // Note that this means its possible to have two different
//             // descriptors match when there is a hash collision so we
//             // should button this up later.
//             return ctx.hash(a) == ctx.hash(b);
//         }
//     },
//     std.hash_map.default_max_load_percentage,
// );
//
// PORT NOTE: the reference's custom context hashes by `descriptor.hashcode()` and
// treats equal hashes as equal keys (the documented hash-collision behavior in the
// `eql` comment above). Modeled here as a `HashMap<u64, Option<Index>>` keyed
// directly by the hashcode, reproducing that "same hashcode == same key" semantics.
#[derive(Default)]
pub struct DescriptorCache(HashMap<u64, Option<collection::Index>>);

impl DescriptorCache {
    fn get(&self, desc: &DiscoveryDescriptor) -> Option<Option<collection::Index>> {
        self.0.get(&desc.hashcode()).copied()
    }

    fn put(&mut self, desc: DiscoveryDescriptor, idx: Option<collection::Index>) {
        self.0.insert(desc.hashcode(), idx);
    }

    // pub fn deinit(self, alloc) — the `HashMap` frees itself.
    fn deinit(&mut self) {
        self.0.clear();
    }
}

/// Bridge `crate::Presentation` -> the sprite face's `Presentation`. In the
/// reference these are the same `font.Presentation`; rustty's below-vt sprite
/// leaf carries a local mirror, so `sprite.hasCodepoint` needs this conversion.
fn to_sprite_presentation(p: Presentation) -> rustty_font::sprite::face::Presentation {
    match p {
        Presentation::Text => rustty_font::sprite::face::Presentation::Text,
        Presentation::Emoji => rustty_font::sprite::face::Presentation::Emoji,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collection::{AddOptions, LoadOptions, SizeAdjustment};
    use crate::face::{DesiredSize, Face, Options};
    use crate::library::Library;
    use crate::options;
    use allocator_api2::alloc::Global;
    use rustty_font::atlas::Format;
    use rustty_font::embedded;
    use rustty_font::metrics::Metrics;

    // PORT NOTE: Zig `try` setup calls become `.unwrap()`; `try testing.expectXxx`
    // become `assert!`/`assert_eq!`. The resolver is built `.{}`-style: fields not
    // named default (`styles = all true`, `discover = null`, `codepoint_map =
    // null`, `descriptor_cache = {}`, `sprite = null`).

    fn opts(size: DesiredSize) -> Options {
        Options {
            size,
            freetype_load_flags: crate::face::FREETYPE_LOAD_FLAGS_DEFAULT,
        }
    }

    // test getIndex
    #[test]
    fn get_index() {
        let alloc = Global;
        let test_font = embedded::REGULAR;
        let test_emoji = embedded::EMOJI;
        let test_emoji_text = embedded::EMOJI_TEXT;

        let mut lib = Library::init().unwrap();

        let mut c = Collection::init();
        c.load_options = Some(LoadOptions {
            library: lib,
            ..Default::default()
        });

        {
            // errdefer c.deinit(alloc);
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
            // if (comptime !font.options.backend.hasCoretext()) {
            //     // Coretext doesn't support Noto's format
            //     _ = try c.add(alloc, try .init(lib, testEmoji, ...), ...);
            // }
            if !options::BACKEND.has_coretext() {
                #[cfg(not(target_os = "macos"))]
                c.add(
                    Face::init(
                        lib,
                        test_emoji,
                        opts(DesiredSize {
                            points: 12.0,
                            xdpi: crate::face::DEFAULT_DPI,
                            ydpi: crate::face::DEFAULT_DPI,
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
            let _ = test_emoji;
            c.add(
                Face::init(
                    lib,
                    test_emoji_text,
                    opts(DesiredSize {
                        points: 12.0,
                        xdpi: crate::face::DEFAULT_DPI,
                        ydpi: crate::face::DEFAULT_DPI,
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

        let mut r = CodepointResolver {
            collection: c,
            styles: StyleStatus::default(),
            discover: None,
            codepoint_map: None,
            descriptor_cache: DescriptorCache::default(),
            sprite: None,
        };

        // Should find all visible ASCII
        let mut i: u32 = 32;
        while i < 127 {
            let idx = r.get_index(alloc, i, Style::Regular, None).unwrap();
            assert_eq!(Style::Regular, idx.style());
            assert_eq!(0u16, idx.idx());
            i += 1;
        }

        // Try emoji
        {
            let idx = r
                .get_index(alloc, u32::from('🥸'), Style::Regular, None)
                .unwrap();
            assert_eq!(Style::Regular, idx.style());
            assert_eq!(1u16, idx.idx());
        }

        // Try text emoji
        {
            let idx = r
                .get_index(alloc, 0x270C, Style::Regular, Some(Presentation::Text))
                .unwrap();
            assert_eq!(Style::Regular, idx.style());
            // const text_idx = if (comptime font.options.backend.hasCoretext()) 1 else 2;
            let text_idx: u16 = if options::BACKEND.has_coretext() {
                1
            } else {
                2
            };
            assert_eq!(text_idx, idx.idx());
        }
        {
            let idx = r
                .get_index(alloc, 0x270C, Style::Regular, Some(Presentation::Emoji))
                .unwrap();
            assert_eq!(Style::Regular, idx.style());
            assert_eq!(1u16, idx.idx());
        }

        // Box glyph should be null since we didn't set a box font
        {
            assert!(r.get_index(alloc, 0x1FB00, Style::Regular, None).is_none());
        }

        r.deinit();
        lib.deinit();
    }

    // test "getIndex disabled font style"
    #[test]
    fn get_index_disabled_font_style() {
        let alloc = Global;
        let test_font = embedded::REGULAR;

        let atlas_grayscale = Atlas::init(alloc, 512, Format::Grayscale).unwrap();

        let mut lib = Library::init().unwrap();

        let mut c = Collection::init();
        c.load_options = Some(LoadOptions {
            library: lib,
            ..Default::default()
        });

        for style in [Style::Regular, Style::Bold, Style::Italic] {
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
                    style,
                    fallback: false,
                    size_adjustment: SizeAdjustment::None,
                },
            )
            .unwrap();
        }

        let mut r = CodepointResolver {
            collection: c,
            styles: StyleStatus::default(),
            discover: None,
            codepoint_map: None,
            descriptor_cache: DescriptorCache::default(),
            sprite: None,
        };
        r.styles.set(Style::Bold, false); // Disable bold

        // Regular should work fine
        {
            let idx = r
                .get_index(alloc, u32::from('A'), Style::Regular, None)
                .unwrap();
            assert_eq!(Style::Regular, idx.style());
            assert_eq!(0u16, idx.idx());
        }

        // Bold should go to regular
        {
            let idx = r
                .get_index(alloc, u32::from('A'), Style::Bold, None)
                .unwrap();
            assert_eq!(Style::Regular, idx.style());
            assert_eq!(0u16, idx.idx());
        }

        // Italic should still work
        {
            let idx = r
                .get_index(alloc, u32::from('A'), Style::Italic, None)
                .unwrap();
            assert_eq!(Style::Italic, idx.style());
            assert_eq!(0u16, idx.idx());
        }

        r.deinit();
        atlas_grayscale.deinit(alloc);
        lib.deinit();
    }

    // test "getIndex box glyph"
    #[test]
    fn get_index_box_glyph() {
        let alloc = Global;

        let mut lib = Library::init().unwrap();

        let c = Collection::init();

        let mut r = CodepointResolver {
            collection: c,
            styles: StyleStatus::default(),
            discover: None,
            codepoint_map: None,
            descriptor_cache: DescriptorCache::default(),
            sprite: Some(SpriteFace {
                metrics: Metrics::calc(rustty_font::metrics::FaceMetrics {
                    px_per_em: 30.0,
                    cell_width: 18.0,
                    ascent: 30.0,
                    descent: -6.0,
                    line_gap: 0.0,
                    // (reference sets only the five above; the remaining
                    // `FaceMetrics` fields default to null.)
                    underline_position: None,
                    underline_thickness: None,
                    strikethrough_position: None,
                    strikethrough_thickness: None,
                    cap_height: None,
                    ex_height: None,
                    ascii_height: None,
                    ic_width: None,
                }),
            }),
        };

        // Should find a box glyph
        let idx = r.get_index(alloc, 0x2500, Style::Regular, None).unwrap();
        assert_eq!(Style::Regular, idx.style());
        // try testing.expectEqual(@intFromEnum(Collection.Index.Special.sprite), idx.idx);
        assert_eq!(collection::Index::SPECIAL_START, idx.idx());

        r.deinit();
        let _ = lib;
        lib.deinit();
    }
}
