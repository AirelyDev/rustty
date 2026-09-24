//! This structure contains a set of SharedGrid structures keyed by
//! unique font configuration.
//!
//! Most terminals (surfaces) will share the same font configuration.
//! This structure allows expensive font information such as
//! the font atlas, glyph cache, font faces, etc. to be shared.
//!
//! This structure is thread-safe when the operations are documented
//! as thread-safe.
// const SharedGridSet = @This();

// const std = @import("std");
// const builtin = @import("builtin");
// const assert = @import("../quirks.zig").inlineAssert;
// const Allocator = std.mem.Allocator;
// const ArenaAllocator = std.heap.ArenaAllocator;
// const font = @import("main.zig");
// const CodepointResolver = font.CodepointResolver;
// const Collection = font.Collection;
// const Discover = font.Discover;
// const Style = font.Style;
// const Library = font.Library;
// const Metrics = font.Metrics;
// const CodepointMap = font.CodepointMap;
// const DesiredSize = font.face.DesiredSize;
// const Face = font.Face;
// const SharedGrid = font.SharedGrid;
// const discovery = @import("discovery.zig");
// const configpkg = @import("../config.zig");
// const Config = configpkg.Config;
//
// PORT NOTE (imports): `configpkg` is the local `crate::config` DI stand-in (break
// #9 — the real `rustty-config` is a HIGHER crate; see `config.rs`). `ArenaAllocator`
// is dropped: rustty models the derived-config/key allocations as owned `Vec`s that
// free themselves (RAII), so no arena is threaded. `builtin.target.os.tag.isDarwin()`
// → `cfg!(target_os = "macos")`, and `Discover != void` is
// `!discovery::DISCOVER_IS_VOID` (false only on the `.freetype`/`.web_canvas`
// "no discovery" backends; S-14, N266 — it was folded to `true`).
use crate::codepoint_resolver::{CodepointResolver, StyleStatus};
use crate::collection::{self, AddOptions, Collection, LoadOptions, SizeAdjustment};
use crate::config::{FontStyle, RepeatableCodepointMap, RepeatableFontVariation, RepeatableString};
use crate::discovery::{self, Descriptor};
use crate::face::{DesiredSize, Face, Id, Variation};
use crate::library::Library;
use crate::shared_grid::{InitError as GridInitError, SharedGrid};
use crate::{Discover, Style};
use allocator_api2::alloc::{Allocator, Global};
use rustty_font::codepoint_map::CodepointMap;
use rustty_font::metrics::{self, Modifier, ModifierSet};
use std::collections::HashMap;

// const log = std.log.scoped(.font_shared_grid_set);
//
// The reference's `std.log.scoped(.font_shared_grid_set)` tag, carried through
// the `log` facade's `target:` (`.port/parity-closure/DECISION-logging.md` §6).
const LOG_SCOPE: &str = "font_shared_grid_set";

/// The reference logs `field.name` from `@typeInfo(Style).@"enum".fields`, i.e.
/// the Zig enum tag spelling. Rust's `Style` variants are `CamelCase`, so the
/// oracle's exact tag text is reproduced here rather than derived from `Debug`.
fn style_field_name(style: Style) -> &'static str {
    match style {
        Style::Regular => "regular",
        Style::Bold => "bold",
        Style::Italic => "italic",
        Style::BoldItalic => "bold_italic",
    }
}

/// SharedGridSet: see the module doc comment.
// PORT NOTE: generic over the allocator `A` the grids/atlases use (`= Global`);
// the reference stores a bare `alloc: Allocator`.
pub struct SharedGridSet<A: Allocator + Copy = Global> {
    /// The allocator to use for all heap allocations.
    alloc: A,

    /// The map of font configurations to SharedGrid instances.
    // map: Map = .{},
    map: Map<A>,

    /// The font library that is used for all font groups.
    font_lib: Library,

    /// Font discovery mechanism.
    // font_discover: ?Discover = null,
    font_discover: Option<Discover>,

    /// Lock to protect multi-threaded access to the map.
    // lock: std.Thread.Mutex = .{},
    lock: std::sync::Mutex<()>,
}

/// `pub const InitError = Library.InitError;`
// PORT NOTE: the backend `Library`'s init error. On the macOS coretext backend
// `Library` is the `NoopLibrary` whose `InitError` is the uninhabited
// `LibraryError` (init never fails); on the freetype backend it is `ft::FtError`.
// Keyed on `hasFreetype()` (the `Library` selector's own switch): the
// non-FreeType backends (coretext, coretext_harfbuzz, coretext_noshape,
// web_canvas) → `NoopLibrary` whose `InitError` is the uninhabited
// `LibraryError` (init never fails); the FreeType backends (including
// `coretext_freetype` on macOS) → `ft::FtError`.
#[cfg(not(rustty_font_has_freetype))]
pub type InitError = crate::library::LibraryError;
#[cfg(rustty_font_has_freetype)]
pub type InitError = crate::ft::FtError;

/// The combined error set for `ref`/`collection` (the reference's inferred error
/// unions of `Key.init`, `collection`, and `SharedGrid.init`).
#[derive(Debug)]
pub enum Error {
    OutOfMemory,
    Collection(collection::Error),
    Face(crate::face::FaceError),
    Discovery(discovery::Error),
    GridInit(GridInitError),
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
impl From<discovery::Error> for Error {
    fn from(e: discovery::Error) -> Self {
        Error::Discovery(e)
    }
}
impl From<GridInitError> for Error {
    fn from(e: GridInitError) -> Self {
        Error::GridInit(e)
    }
}

impl SharedGridSet<Global> {
    /// Initialize a new SharedGridSet.
    // pub fn init(alloc: Allocator) InitError!SharedGridSet {
    pub fn init(alloc: Global) -> Result<SharedGridSet<Global>, InitError> {
        let mut font_lib = Library::init()?;
        // errdefer font_lib.deinit(); — on the (uninhabited on macOS) error path.
        let _ = &mut font_lib;

        Ok(SharedGridSet {
            alloc,
            map: HashMap::new(),
            font_lib,
            font_discover: None,
            lock: std::sync::Mutex::new(()),
        })
    }
}

impl<A: Allocator + Copy> SharedGridSet<A> {
    // pub fn deinit(self: *SharedGridSet) void {
    pub fn deinit(&mut self) {
        // var it = self.map.iterator();
        // while (it.next()) |entry| { entry.key_ptr.deinit(); v.grid.deinit(alloc); alloc.destroy(v.grid); }
        for (_hashcode, v) in self.map.drain() {
            // entry.key_ptr.deinit(); — the `Key` in the map value; its owned data
            // frees on drop.
            // v.grid.deinit(self.alloc); alloc.destroy(v.grid);
            // SAFETY: `v.grid` was produced by `Box::into_raw`; reclaim + drop it.
            let mut grid = unsafe { Box::from_raw(v.grid) };
            grid.deinit();
            drop(grid);
        }
        // self.map.deinit(self.alloc); — `HashMap` frees on drop.

        // if (comptime Discover != void) { if (self.font_discover) |*v| v.deinit(); }
        if !discovery::DISCOVER_IS_VOID {
            if let Some(v) = self.font_discover.as_mut() {
                v.deinit();
            }
        }

        self.font_lib.deinit();
    }

    /// Returns the number of cached grids.
    // pub fn count(self: *SharedGridSet) usize {
    pub fn count(&self) -> usize {
        // self.lock.lock(); defer self.lock.unlock();
        let _guard = self.lock.lock().unwrap();
        self.map.len()
    }

    /// Initialize a SharedGrid for the given font configuration. If the
    /// SharedGrid is not present it will be initialized with a ref count of
    /// 1. If it is present, the ref count will be incremented.
    ///
    /// This is NOT thread-safe.
    ///
    /// The returned data (key and grid) should never be freed. The memory is
    /// owned by the set and will be freed when the ref count reaches zero.
    // pub fn ref(self, config, font_size) !struct { Key, *SharedGrid } {
    //
    // PORT NOTE: `ref` is a Rust keyword; the method keeps the reference name via a
    // raw identifier (`r#ref`). The reference returns `gop.key_ptr.*` — a value copy
    // of the stored `Key` that shares its arena with the map's key; rustty instead
    // returns the freshly-built (owned) `Key`, which has identical content and thus
    // the same `hashcode` (all `deref` needs). The map is keyed by that `hashcode`.
    #[allow(clippy::type_complexity)]
    pub fn r#ref(
        &mut self,
        config: &DerivedConfig,
        font_size: DesiredSize,
    ) -> Result<(Key, *mut SharedGrid<A>), Error> {
        // var key = try Key.init(self.alloc, config, font_size);
        let key = Key::init(config, font_size);
        // errdefer key.deinit(); — owned data frees on drop.

        // self.lock.lock(); defer self.lock.unlock();
        let _guard = self.lock.lock().unwrap();

        let hashcode = key.hashcode();

        // const gop = try self.map.getOrPut(self.alloc, key);
        // if (gop.found_existing) { ... }
        if let Some(reffed) = self.map.get_mut(&hashcode) {
            // log.debug("found cached grid for font config", .{});
            log::debug!(target: LOG_SCOPE, "found cached grid for font config");

            // We can deinit the key because we found a cached value.
            // key.deinit(); — the local key is returned instead (see PORT NOTE).

            // Increment our ref count and return the cache
            // gop.value_ptr.ref += 1;
            reffed.r#ref += 1;
            // return .{ gop.key_ptr.*, gop.value_ptr.grid };
            let grid = reffed.grid;
            return Ok((key, grid));
        }
        // errdefer self.map.removeByPtr(gop.key_ptr);

        // log.debug("initializing new grid for font config", .{});
        log::debug!(target: LOG_SCOPE, "initializing new grid for font config");

        // A new font config, initialize the cache.
        // const grid = try self.alloc.create(SharedGrid);
        // errdefer self.alloc.destroy(grid);
        //
        // PORT NOTE: the reference allocates the grid first (stable pointer), then
        // fills it via `grid.* = try .init(...)`. rustty builds the resolver +
        // grid, boxes it (stable heap pointer), and inserts the raw pointer. If
        // `collection`/`init` fails, the local values drop (freeing everything),
        // and nothing is inserted — matching the reference's `errdefer` unwind.

        // grid.* = try .init(self.alloc, resolver: { ... });
        let grid_value = {
            // Build our collection. This is the expensive operation that
            // involves finding fonts, loading them (maybe, some are deferred),
            // etc.
            // PORT NOTE: `collection`/`discover` take explicit field borrows (not
            // `&mut self`) so they compose with the held `lock` guard (which borrows
            // `self.lock`); Rust's disjoint-field borrows then permit touching
            // `self.font_discover`/`self.map` under the same guard, faithfully
            // keeping the reference's whole-`ref` critical section locked.
            let c = Self::collection(
                &mut self.font_discover,
                self.font_lib,
                &key,
                font_size,
                config,
            )?;
            // errdefer c.deinit(self.alloc); — `collection` deinits the
            // partially-built collection itself before its error propagates
            // through this `?` (W9F-02); after it returns, nothing fallible
            // runs before `c` is moved into the resolver below (the reference
            // errdefer's remaining coverage, `try self.discover()`, maps to the
            // infallible `discover_impl`).

            // Setup our enabled/disabled styles
            let mut styles = StyleStatus::init_fill(true);
            // styles.set(.bold, config.@"font-style-bold" != .false);
            styles.set(Style::Bold, config.font_style_bold != FontStyle::False);
            styles.set(Style::Italic, config.font_style_italic != FontStyle::False);
            styles.set(
                Style::BoldItalic,
                config.font_style_bold_italic != FontStyle::False,
            );

            // Init our resolver which just requires setting fields.
            let resolver = CodepointResolver {
                collection: c,
                styles,
                discover: discover_impl(&mut self.font_discover, self.font_lib),
                codepoint_map: Some(key.codepoint_map.clone(Global).unwrap()),
                descriptor_cache: Default::default(),
                sprite: None,
            };

            let alloc = self.alloc;
            SharedGrid::init(alloc, resolver)?
        };
        // errdefer grid.deinit(self.alloc);

        let grid: *mut SharedGrid<A> = Box::into_raw(Box::new(grid_value));

        // gop.value_ptr.* = .{ .grid = grid, .ref = 1 };
        self.map.insert(hashcode, ReffedGrid { grid, r#ref: 1 });

        // return .{ gop.key_ptr.*, gop.value_ptr.grid };
        Ok((key, grid))
    }

    /// Builds the Collection for the given configuration key and
    /// initial font size.
    // fn collection(self, key, size, config) !Collection {
    //
    // PORT NOTE: takes explicit `font_discover`/`font_lib` field borrows rather than
    // `&mut self` (see `ref`'s PORT NOTE) so it composes with the held `lock` guard.
    fn collection(
        font_discover: &mut Option<Discover>,
        font_lib: Library,
        key: &Key,
        size: DesiredSize,
        config: &DerivedConfig,
    ) -> Result<Collection, Error> {
        // A quick note on memory management:
        // - font_lib is owned by the SharedGridSet
        // - metric_modifiers is owned by the key which is freed only when
        //   the ref count for this grid reaches zero.
        let load_options: LoadOptions = LoadOptions {
            library: font_lib,
            size,
            freetype_load_flags: key.freetype_load_flags,
        };

        let mut c = Collection::init();
        // errdefer c.deinit(self.alloc);
        //
        // PORT NOTE (W9F-02): `Collection` has no `Drop` — its entries own raw
        // native handles torn down only by `deinit` — so the reference errdefer
        // is materialized by running the fallible remainder in
        // `collection_fill` and deiniting `c` before propagating its error.
        c.load_options = Some(load_options);
        c.metric_modifiers = key.metric_modifiers.clone();

        match Self::collection_fill(&mut c, font_discover, font_lib, key, config, load_options) {
            Ok(()) => Ok(c),
            Err(e) => {
                // The materialized `errdefer c.deinit(self.alloc)`
                // (SharedGridSet.zig:175).
                c.deinit();
                Err(e)
            }
        }
    }

    /// The fallible remainder of `collection` — everything the reference runs
    /// under `errdefer c.deinit(self.alloc)` — split out so the error path can
    /// deinit the partially-built collection (W9F-02).
    fn collection_fill(
        c: &mut Collection,
        font_discover: &mut Option<Discover>,
        font_lib: Library,
        key: &Key,
        config: &DerivedConfig,
        load_options: LoadOptions,
    ) -> Result<(), Error> {
        // Search for fonts
        // if (Discover != void) discover: { ... }
        'discover: {
            if discovery::DISCOVER_IS_VOID {
                break 'discover;
            }
            // const disco = try self.discover() orelse { log.warn(...); break :discover; };
            let disco = match discover_impl(font_discover, font_lib) {
                Some(d) => d,
                None => {
                    // log.warn("font discovery not available, cannot search for fonts", .{});
                    log::warn!(
                        target: LOG_SCOPE,
                        "font discovery not available, cannot search for fonts"
                    );
                    break 'discover;
                }
            };
            // SAFETY: `disco` aliases `self.font_discover`, which is not mutated for
            // the duration of this block.
            let disco = unsafe { &mut *disco };

            // A buffer we use to store the font names for logging.
            let mut name_buf = [0u8; 256];

            // inline for (@typeInfo(Style).@"enum".fields) |field| {
            for style in [
                Style::Regular,
                Style::Bold,
                Style::Italic,
                Style::BoldItalic,
            ] {
                // for (key.descriptorsForStyle(style)) |desc| {
                for desc in key.descriptors_for_style(style) {
                    {
                        // var disco_it = try disco.discover(self.alloc, desc);
                        // defer disco_it.deinit(); — RAII: `disco_it` drops at block/continue.
                        let mut disco_it = disco.discover(desc.clone())?;
                        // if (try disco_it.next()) |face| { ... continue; }
                        if let Some(face) = disco_it.next()? {
                            // log.info("font {s}: {s}", .{ field.name, try face.name(&name_buf) });
                            //
                            // PORT NOTE: `{s}` renders the raw name bytes; the
                            // lossy UTF-8 conversion is confined to the message.
                            log::info!(
                                target: LOG_SCOPE,
                                "font {}: {}",
                                style_field_name(style),
                                String::from_utf8_lossy(face.name(&mut name_buf)?)
                            );

                            // _ = try c.addDeferred(self.alloc, face, .{ .style, .fallback=false, .size_adjustment=.none });
                            c.add_deferred(
                                face,
                                AddOptions {
                                    style,
                                    fallback: false,
                                    // No size adjustment for primary fonts.
                                    size_adjustment: SizeAdjustment::None,
                                },
                            )?;

                            continue;
                        }
                    }

                    // If there are variation configurations and we didn't find
                    // the font, then we retry the discovery with all stylistic
                    // bits set to false. This is because some fonts may not
                    // set the stylistic bit in their table but still support
                    // axes to mimic the style. At the time of writing, Berkeley
                    // Mono Variable is like this. See #2140.
                    // if (style != .regular and desc.variations.len > 0) {
                    if style != Style::Regular && !desc.variations.is_empty() {
                        // var disco_it = try disco.discover(self.alloc, desc: { var copy = desc; copy.bold=false; copy.italic=false; break :desc copy; });
                        let mut copy = desc.clone();
                        copy.bold = false;
                        copy.italic = false;
                        // defer disco_it.deinit(); — RAII scope drop.
                        let mut disco_it = disco.discover(copy)?;
                        // if (try disco_it.next()) |face| { ... continue; }
                        if let Some(face) = disco_it.next()? {
                            // log.info("font {s}: {s}", .{ field.name, try face.name(&name_buf) });
                            //
                            // PORT NOTE: `{s}` renders the raw name bytes; the
                            // lossy UTF-8 conversion is confined to the message.
                            log::info!(
                                target: LOG_SCOPE,
                                "font {}: {}",
                                style_field_name(style),
                                String::from_utf8_lossy(face.name(&mut name_buf)?)
                            );

                            c.add_deferred(
                                face,
                                AddOptions {
                                    style,
                                    fallback: false,
                                    // No size adjustment for primary fonts.
                                    size_adjustment: SizeAdjustment::None,
                                },
                            )?;

                            continue;
                        }
                    }

                    // log.warn("font-family {s} not found: {s}", .{ field.name, desc.family.? });
                    //
                    // PORT NOTE: `desc.family.?` unwraps unconditionally in the
                    // reference; `expect` reproduces that (this arm is only
                    // reachable for a family-bearing descriptor). `{s}` renders
                    // raw bytes; the lossy conversion is message-only.
                    log::warn!(
                        target: LOG_SCOPE,
                        "font-family {} not found: {}",
                        style_field_name(style),
                        String::from_utf8_lossy(
                            desc.family.as_deref().expect("desc.family.?")
                        )
                    );
                }
            }
        }

        // Complete our styles to ensure we have something to satisfy every
        // possible style request. We do this before adding our built-in font
        // because we want to ensure our built-in styles are fallbacks to
        // the configured styles.
        // try c.completeStyles(self.alloc, config.@"font-synthetic-style");
        c.complete_styles(config.font_synthetic_style)?;

        // Our built-in font will be used as a backup
        // _ = try c.add(self.alloc, try .init(font_lib, font.embedded.variable, load_options.faceOptions()), .{ .style=.regular, .fallback=true, .size_adjustment=font.default_fallback_adjustment });
        c.add(
            Face::init(
                font_lib,
                embedded_face_source(rustty_font::embedded::VARIABLE),
                load_options.face_options(),
            )?,
            AddOptions {
                style: Style::Regular,
                fallback: true,
                size_adjustment: crate::DEFAULT_FALLBACK_ADJUSTMENT,
            },
        )?;
        // try (try c.getFace(try c.add(self.alloc, try .init(...variable...), .{ .style=.bold, .fallback=true, .size_adjustment=... }))).setVariations(&.{.{ .id = .init("wght"), .value = 700 }}, load_options.faceOptions());
        {
            let idx = c.add(
                Face::init(
                    font_lib,
                    embedded_face_source(rustty_font::embedded::VARIABLE),
                    load_options.face_options(),
                )?,
                AddOptions {
                    style: Style::Bold,
                    fallback: true,
                    size_adjustment: crate::DEFAULT_FALLBACK_ADJUSTMENT,
                },
            )?;
            let face = c.get_face(idx)?;
            unsafe { &mut *face }.set_variations(
                &[Variation {
                    id: Id::init(b"wght"),
                    value: 700.0,
                }],
                load_options.face_options(),
            )?;
        }
        // _ = try c.add(self.alloc, try .init(...variable_italic...), .{ .style=.italic, .fallback=true, .size_adjustment=... });
        c.add(
            Face::init(
                font_lib,
                embedded_face_source(rustty_font::embedded::VARIABLE_ITALIC),
                load_options.face_options(),
            )?,
            AddOptions {
                style: Style::Italic,
                fallback: true,
                size_adjustment: crate::DEFAULT_FALLBACK_ADJUSTMENT,
            },
        )?;
        // try (try c.getFace(try c.add(...variable_italic..., .style=.bold_italic...))).setVariations(&.{.{ .id = .init("wght"), .value = 700 }}, ...);
        {
            let idx = c.add(
                Face::init(
                    font_lib,
                    embedded_face_source(rustty_font::embedded::VARIABLE_ITALIC),
                    load_options.face_options(),
                )?,
                AddOptions {
                    style: Style::BoldItalic,
                    fallback: true,
                    size_adjustment: crate::DEFAULT_FALLBACK_ADJUSTMENT,
                },
            )?;
            let face = c.get_face(idx)?;
            unsafe { &mut *face }.set_variations(
                &[Variation {
                    id: Id::init(b"wght"),
                    value: 700.0,
                }],
                load_options.face_options(),
            )?;
        }

        // Nerd-font symbols fallback.
        // _ = try c.add(self.alloc, try .init(...symbols_nerd_font...), .{ .style=.regular, .fallback=true, .size_adjustment=.none });
        c.add(
            Face::init(
                font_lib,
                embedded_face_source(rustty_font::embedded::SYMBOLS_NERD_FONT),
                load_options.face_options(),
            )?,
            AddOptions {
                style: Style::Regular,
                fallback: true,
                // No size adjustment for the symbols font.
                size_adjustment: SizeAdjustment::None,
            },
        )?;

        // On macOS, always search for and add the Apple Emoji font
        // as our preferred emoji font for fallback. We do this in case
        // people add other emoji fonts to their system, we always want to
        // prefer the official one. Users can override this by explicitly
        // specifying a font-family for emoji.
        // if (comptime builtin.target.os.tag.isDarwin() and Discover != void) apple_emoji: { ... }
        //
        // PORT NOTE (S-14, N266): `Discover != void` is live — it is false for the
        // `.freetype` (no discovery) and `.web_canvas` backends.
        #[cfg(all(
            target_os = "macos",
            not(any(rustty_font_backend = "freetype", rustty_font_backend = "web_canvas"))
        ))]
        'apple_emoji: {
            // const disco = try self.discover() orelse break :apple_emoji;
            let disco = match discover_impl(font_discover, font_lib) {
                Some(d) => d,
                None => break 'apple_emoji,
            };
            // SAFETY: `disco` aliases `self.font_discover`, unmutated for this block.
            let disco = unsafe { &mut *disco };

            // Fast path: we know the exact name of the font we want so we
            // can look it up directly, which is sometimes significantly faster than
            // full discovery (e.g. CoreText).
            // if (@hasDecl(Discover, "discoverExactFamily")) {
            //
            // PORT NOTE: the comptime `@hasDecl` collapses — this block is already
            // macOS-gated, where `Discover = CoreText` always declares
            // `discoverExactFamily`.
            {
                // if (try disco.discoverExactFamily("Apple Color Emoji")) |face| {
                if let Some(face) = disco.discover_exact_family(b"Apple Color Emoji")? {
                    // _ = try c.addDeferred(self.alloc, face, .{
                    //     .style = .regular,
                    //     .fallback = true,
                    //     // No size adjustment for emojis.
                    //     .size_adjustment = .none,
                    // });
                    c.add_deferred(
                        face,
                        AddOptions {
                            style: Style::Regular,
                            fallback: true,
                            // No size adjustment for emojis.
                            size_adjustment: SizeAdjustment::None,
                        },
                    )?;
                    // break :apple_emoji;
                    break 'apple_emoji;
                }
            }

            // var disco_it = try disco.discover(self.alloc, .{ .family = "Apple Color Emoji" });
            // defer disco_it.deinit(); — RAII scope drop.
            let mut disco_it = disco.discover(Descriptor {
                family: Some(b"Apple Color Emoji".to_vec()),
                ..Default::default()
            })?;
            // if (try disco_it.next()) |face| { _ = try c.addDeferred(...); }
            if let Some(face) = disco_it.next()? {
                c.add_deferred(
                    face,
                    AddOptions {
                        style: Style::Regular,
                        fallback: true,
                        // No size adjustment for emojis.
                        size_adjustment: SizeAdjustment::None,
                    },
                )?;
            }
        }

        // Emoji fallback. We don't include this on Mac since Mac is expected
        // to always have the Apple Emoji available on the system.
        // if (comptime !builtin.target.os.tag.isDarwin() or Discover == void) { ... }
        #[cfg(not(all(
            target_os = "macos",
            not(any(rustty_font_backend = "freetype", rustty_font_backend = "web_canvas"))
        )))]
        {
            c.add(
                Face::init(
                    font_lib,
                    embedded_face_source(rustty_font::embedded::EMOJI),
                    load_options.face_options(),
                )?,
                AddOptions {
                    style: Style::Regular,
                    fallback: true,
                    // No size adjustment for emojis.
                    size_adjustment: SizeAdjustment::None,
                },
            )?;
            c.add(
                Face::init(
                    font_lib,
                    embedded_face_source(rustty_font::embedded::EMOJI_TEXT),
                    load_options.face_options(),
                )?,
                AddOptions {
                    style: Style::Regular,
                    fallback: true,
                    // No size adjustment for emojis.
                    size_adjustment: SizeAdjustment::None,
                },
            )?;
        }

        Ok(())
    }

    /// Decrement the ref count for the given key. If the ref count is zero,
    /// the grid will be deinitialized and removed from the map.j:w
    // pub fn deref(self: *SharedGridSet, key: Key) void {
    pub fn deref(&mut self, key: Key) {
        // self.lock.lock(); defer self.lock.unlock();
        let _guard = self.lock.lock().unwrap();

        // const entry = self.map.getEntry(key) orelse return;
        let hashcode = key.hashcode();
        let reffed = match self.map.get_mut(&hashcode) {
            Some(e) => e,
            None => return,
        };
        // assert(entry.value_ptr.ref >= 1);
        assert!(reffed.r#ref >= 1);

        // If we have more than one reference, decrement and return.
        if reffed.r#ref > 1 {
            reffed.r#ref -= 1;
            return;
        }

        // We are at a zero ref count so deinit the group and remove.
        // entry.key_ptr.deinit(); — the stored key frees on removal below.
        // entry.value_ptr.grid.deinit(self.alloc); alloc.destroy(entry.value_ptr.grid);
        // self.map.removeByPtr(entry.key_ptr);
        let reffed = self.map.remove(&hashcode).unwrap();
        // SAFETY: `grid` came from `Box::into_raw`; reclaim and drop it (freeing it).
        let mut grid = unsafe { Box::from_raw(reffed.grid) };
        grid.deinit();
        drop(grid);
        // `key` (the argument) frees its owned data on drop here.
    }
}

/// Initialize once and return the font discovery mechanism. This remains
/// initialized throughout the lifetime of the application because some
/// font discovery mechanisms (i.e. fontconfig) are unsafe to reinit.
// fn discover(self: *SharedGridSet) !?*Discover {
//
// PORT NOTE: the reference method returns `!?*Discover`, but the body never errors
// (backend `Discover.init` is infallible in rustty), so this returns
// `Option<*mut Discover>`. The `Discover == void` early-return is live
// (`discovery::DISCOVER_IS_VOID`; S-14, N266 — it was elided while every native
// build had a concrete discoverer). It is a free
// function over the `font_discover`/`font_lib` fields (not a `&mut self` method) so
// callers can invoke it while holding the `lock` guard (see `ref`'s PORT NOTE).
fn discover_impl(font_discover: &mut Option<Discover>, font_lib: Library) -> Option<*mut Discover> {
    // If we're built without a font discovery mechanism, return null
    // if (comptime Discover == void) return null;
    if discovery::DISCOVER_IS_VOID {
        return None;
    }

    // If we initialized, use it
    // if (self.font_discover) |*v| return v;
    if font_discover.is_some() {
        return Some(font_discover.as_mut().unwrap() as *mut Discover);
    }

    // self.font_discover = .init(self.font_lib);
    *font_discover = Some(Discover::init(font_lib));
    // return &self.font_discover.?;
    Some(font_discover.as_mut().unwrap() as *mut Discover)
}

/// The `source` handed to `Face::init` for the BUILT-IN (embedded) fallback
/// faces — the reference's `font.embedded.variable` / `…variable_italic` /
/// `…symbols_nerd_font` / `…emoji` / `…emoji_text` byte slices.
///
/// PORT NOTE (wasm divergence): the reference `SharedGridSet.zig` is never
/// instantiated with the `web_canvas` face (it has no wasm section, and Zig would
/// reject `Face.init(lib, bytes, opts)` at compile time because the browser face
/// has no bytes-based initializer). In rustty `face::Face` IS `web_canvas::Face`
/// on `wasm32`, so these call sites would hand raw TTF bytes to a face that
/// interprets `source` as a CSS "font" string — which the browser rejects,
/// silently keeping its previous/fallback font (and now, since the face validates
/// its input, fails loudly with `Error::InvalidFontStr`). The browser has no way
/// to consume our embedded font files here, so on wasm the built-in fallbacks
/// resolve to the CSS generic family the browser picks for them; the embedded
/// bytes stay unused. Native targets pass the bytes through unchanged.
#[cfg(not(target_arch = "wasm32"))]
fn embedded_face_source(bytes: &'static [u8]) -> &'static [u8] {
    bytes
}

/// See the `cfg(not(wasm32))` twin above.
#[cfg(target_arch = "wasm32")]
fn embedded_face_source(_bytes: &'static [u8]) -> &'static [u8] {
    b"monospace"
}

/// Map of font configurations to grid instances. The grid
/// instances are pointers that are heap allocated so that they're
/// stable pointers across hash map resizes.
// pub const Map = std.HashMapUnmanaged(Key, ReffedGrid, struct { hash=k.hashcode(); eql=hash(a)==hash(b); }, ...);
//
// PORT NOTE: the reference keys the map by `Key`, with a context that hashes by
// `key.hashcode()` and treats equal hashes as equal keys (the same "same hashcode
// == same key" semantics as `CodepointResolver`'s descriptor cache). rustty models
// this as `HashMap<u64, ReffedGrid>` keyed directly by the hashcode, reproducing
// that semantics exactly.
type Map<A> = HashMap<u64, ReffedGrid<A>>;

/// Ref-counted SharedGrid.
// const ReffedGrid = struct { grid: *SharedGrid, ref: u32 = 0 };
struct ReffedGrid<A: Allocator + Copy> {
    grid: *mut SharedGrid<A>,
    // PORT NOTE: `ref` is a Rust keyword; the raw identifier keeps the name.
    r#ref: u32,
}

/// This is the configuration required to create a key without having
/// to keep the full Ghostty configuration around.
// pub const DerivedConfig = struct { arena, @"font-family", ... };
//
// PORT NOTE: the reference `arena: ArenaAllocator` is dropped — the cloned config
// values are modeled as owned fields that free on drop (RAII). `Config` (the giant
// global config) is only ever read for its font subset via `DerivedConfig`, which
// in rustty IS that subset, so `Config` aliases `DerivedConfig` (see below).
#[derive(Debug, Default)]
pub struct DerivedConfig {
    pub font_family: RepeatableString,
    pub font_family_bold: RepeatableString,
    pub font_family_italic: RepeatableString,
    pub font_family_bold_italic: RepeatableString,
    pub font_style: FontStyle,
    pub font_style_bold: FontStyle,
    pub font_style_italic: FontStyle,
    pub font_style_bold_italic: FontStyle,
    pub font_variation: RepeatableFontVariation,
    pub font_variation_bold: RepeatableFontVariation,
    pub font_variation_italic: RepeatableFontVariation,
    pub font_variation_bold_italic: RepeatableFontVariation,
    pub font_codepoint_map: RepeatableCodepointMap,
    pub font_synthetic_style: crate::collection::FontSyntheticStyle,
    pub adjust_cell_width: Option<Modifier>,
    pub adjust_cell_height: Option<Modifier>,
    pub adjust_font_baseline: Option<Modifier>,
    pub adjust_underline_position: Option<Modifier>,
    pub adjust_underline_thickness: Option<Modifier>,
    pub adjust_strikethrough_position: Option<Modifier>,
    pub adjust_strikethrough_thickness: Option<Modifier>,
    pub adjust_overline_position: Option<Modifier>,
    pub adjust_overline_thickness: Option<Modifier>,
    pub adjust_cursor_thickness: Option<Modifier>,
    pub adjust_cursor_height: Option<Modifier>,
    pub adjust_box_thickness: Option<Modifier>,
    pub adjust_icon_height: Option<Modifier>,
    pub freetype_load_flags: crate::face::FreetypeLoadFlags,
}

/// `configpkg.Config` — see the `DerivedConfig` PORT NOTE (the modeled font subset
/// of the reference global config is exactly `DerivedConfig`).
pub type Config = DerivedConfig;

impl DerivedConfig {
    /// Initialize a DerivedConfig. The config should be either a
    /// config.Config or another DerivedConfig to clone from.
    // pub fn init(alloc_gpa: Allocator, config: anytype) Allocator.Error!DerivedConfig {
    //
    // PORT NOTE: the reference clones each field into an arena. rustty clones into
    // owned fields (RAII); allocation aborts on OOM, so `init` is infallible. Since
    // the modeled `Config` and `DerivedConfig` are the same type, the `anytype`
    // source is `&DerivedConfig`.
    pub fn init(config: &DerivedConfig) -> DerivedConfig {
        DerivedConfig {
            font_family: config.font_family.clone(),
            font_family_bold: config.font_family_bold.clone(),
            font_family_italic: config.font_family_italic.clone(),
            font_family_bold_italic: config.font_family_bold_italic.clone(),
            font_style: config.font_style.clone(),
            font_style_bold: config.font_style_bold.clone(),
            font_style_italic: config.font_style_italic.clone(),
            font_style_bold_italic: config.font_style_bold_italic.clone(),
            font_variation: config.font_variation.clone(),
            font_variation_bold: config.font_variation_bold.clone(),
            font_variation_italic: config.font_variation_italic.clone(),
            font_variation_bold_italic: config.font_variation_bold_italic.clone(),
            font_codepoint_map: config.font_codepoint_map.clone_config(),
            font_synthetic_style: config.font_synthetic_style,
            adjust_cell_width: config.adjust_cell_width,
            adjust_cell_height: config.adjust_cell_height,
            adjust_font_baseline: config.adjust_font_baseline,
            adjust_underline_position: config.adjust_underline_position,
            adjust_underline_thickness: config.adjust_underline_thickness,
            adjust_strikethrough_position: config.adjust_strikethrough_position,
            adjust_strikethrough_thickness: config.adjust_strikethrough_thickness,
            adjust_overline_position: config.adjust_overline_position,
            adjust_overline_thickness: config.adjust_overline_thickness,
            adjust_cursor_thickness: config.adjust_cursor_thickness,
            adjust_cursor_height: config.adjust_cursor_height,
            adjust_box_thickness: config.adjust_box_thickness,
            adjust_icon_height: config.adjust_icon_height,
            // .@"freetype-load-flags" = if (FreetypeLoadFlags != void) config.@"freetype-load-flags" else {},
            freetype_load_flags: config.freetype_load_flags,
            // This must be last so the arena contains all our allocations
            // from above since Zig does assignment in order.
            // .arena = arena, — PORT NOTE: the arena is dropped (owned fields free on
            // drop); the ordering constraint it documents is moot in rustty.
        }
    }

    /// `Config.default(alloc)` — the reference default configuration (font subset).
    // PORT NOTE: the modeled subset has no non-empty default allocations; the field
    // defaults reproduce the reference (empty families, `.default` styles, empty
    // variations/codepoint-map, all synthetic styles on, no metric adjustments).
    pub fn default_config() -> Config {
        DerivedConfig::default()
    }

    // pub fn deinit(self: *DerivedConfig) void { self.arena.deinit(); }
    // PORT NOTE: owned fields free on drop; no arena.
    pub fn deinit(&mut self) {}
}

/// The key used to uniquely identify a font configuration.
// pub const Key = struct { arena, descriptors, style_offsets, codepoint_map, metric_modifiers, font_size, freetype_load_flags };
//
// PORT NOTE: `arena` is dropped (owned fields free on drop). `descriptors` is an
// owned `Vec` (the reference `[]const discovery.Descriptor` slice lives in the
// arena). `codepoint_map`/`metric_modifiers` are likewise owned.
pub struct Key {
    /// The descriptors used for all the fonts added to the
    /// initial group, including all styles. This is hashed
    /// in order so the order matters. All users of the struct
    /// should ensure that the order is consistent.
    // descriptors: []const discovery.Descriptor = &.{},
    descriptors: Vec<Descriptor>,

    /// These are the offsets into the descriptors array for
    /// each style. For example, bold is from
    /// offsets[@intFromEnum(.bold) - 1] to
    /// offsets[@intFromEnum(.bold)].
    // style_offsets: StyleOffsets = @splat(0),
    style_offsets: StyleOffsets,

    /// The codepoint map configuration.
    // codepoint_map: CodepointMap = .{},
    codepoint_map: CodepointMap,

    /// The metric modifier set configuration.
    // metric_modifiers: Metrics.ModifierSet = .{},
    metric_modifiers: ModifierSet,

    /// The configured font size for this key. We don't use this
    /// directly but it is used as part of the hash for the
    /// font grid.
    // font_size: DesiredSize = .{ .points = 12 },
    font_size: DesiredSize,

    /// The freetype load flags configuration, only non-void if the
    /// freetype backend is enabled.
    // freetype_load_flags: font.face.FreetypeLoadFlags = font.face.freetype_load_flags_default,
    freetype_load_flags: crate::face::FreetypeLoadFlags,
}

// const style_offsets_len = std.enums.directEnumArrayLen(Style, 0);  // 4
// const StyleOffsets = [style_offsets_len]usize;
type StyleOffsets = [usize; 4];

// PORT NOTE (Rust-only glue): Zig value-copies `Key` (Surface.setFontSize's
// `.old_key = self.font_grid_key` / `.new_key = font_grid_key` message
// literal, Surface.zig:2443-2453 — SHALLOW copies whose backing memory
// belongs to the set). The owned rustty `Key` realizes that value-copy as
// `Clone` (deep; `CodepointMap.clone`'s OOM `Result` collapses — infallible
// `Global` allocation, the established convention).
impl Clone for Key {
    fn clone(&self) -> Key {
        Key {
            descriptors: self.descriptors.clone(),
            style_offsets: self.style_offsets,
            codepoint_map: self
                .codepoint_map
                .clone(Global)
                .unwrap_or_else(|_| unreachable!()),
            metric_modifiers: self.metric_modifiers.clone(),
            font_size: self.font_size,
            freetype_load_flags: self.freetype_load_flags,
        }
    }
}

impl Key {
    // comptime { assert(@intFromEnum(Style.regular) == 0); ... }
    // (Verified in `Key::init` as `debug_assert!`s.)

    // pub fn init(alloc_gpa, config_src, font_size) Allocator.Error!Key {
    pub fn init(config_src: &DerivedConfig, font_size: DesiredSize) -> Key {
        // We assume this throughout this structure. If this changes
        // we may need to change this structure.
        debug_assert_eq!(Style::Regular as usize, 0);
        debug_assert_eq!(Style::Bold as usize, 1);
        debug_assert_eq!(Style::Italic as usize, 2);
        debug_assert_eq!(Style::BoldItalic as usize, 3);

        // Clone our configuration. We need to do this because the lifetime
        // of the derived config is usually shorter than that of a key
        // and we use pointers into the derived config for the key. We
        // can remove this if we wanted by dupe-ing the memory we use
        // from DerivedConfig below.
        // var config = try DerivedConfig.init(alloc, config_src);
        let config = DerivedConfig::init(config_src);

        // var descriptors: std.ArrayList(discovery.Descriptor) = .empty;
        let mut descriptors: Vec<Descriptor> = Vec::new();
        // for (config.@"font-family".list.items) |family| {
        for family in &config.font_family.list {
            descriptors.push(Descriptor {
                family: Some(family.clone()),
                style: config.font_style.name_value(),
                size: font_size.points,
                variations: config.font_variation.list.clone(),
                ..Default::default()
            });
        }

        // In all the styled cases below, we prefer to specify an exact
        // style via the `font-style` configuration. If a style is not
        // specified, we use the discovery mechanism to search for a
        // style category such as bold, italic, etc. We can't specify both
        // because the latter will restrict the search to only that. If
        // a user says `font-style = italic` for the bold face for example,
        // no results would be found if we restrict to ALSO searching for
        // italic.
        // for (config.@"font-family-bold".list.items) |family| {
        for family in &config.font_family_bold.list {
            let style = config.font_style_bold.name_value();
            descriptors.push(Descriptor {
                family: Some(family.clone()),
                bold: style.is_none(),
                style,
                size: font_size.points,
                variations: config.font_variation_bold.list.clone(),
                ..Default::default()
            });
        }
        for family in &config.font_family_italic.list {
            let style = config.font_style_italic.name_value();
            descriptors.push(Descriptor {
                family: Some(family.clone()),
                italic: style.is_none(),
                style,
                size: font_size.points,
                variations: config.font_variation_italic.list.clone(),
                ..Default::default()
            });
        }
        for family in &config.font_family_bold_italic.list {
            let style = config.font_style_bold_italic.name_value();
            descriptors.push(Descriptor {
                family: Some(family.clone()),
                bold: style.is_none(),
                italic: style.is_none(),
                style,
                size: font_size.points,
                variations: config.font_variation_bold_italic.list.clone(),
                ..Default::default()
            });
        }

        // Setup the codepoint map
        // const codepoint_map: CodepointMap = map: { if (map.map.list.len == 0) break :map .{}; const clone = try config.@"font-codepoint-map".clone(alloc); break :map clone.map; };
        let codepoint_map: CodepointMap = {
            let map = &config.font_codepoint_map;
            // `if (map.map.list.len == 0)` (verbatim from the reference).
            #[allow(clippy::len_zero)]
            if map.map.len() == 0 {
                CodepointMap::default()
            } else {
                let clone = config.font_codepoint_map.clone_config();
                clone.map
            }
        };

        // Metric modifiers
        // const metric_modifiers: Metrics.ModifierSet = set: { ... };
        let metric_modifiers: ModifierSet = {
            let mut set: ModifierSet = ModifierSet::new();
            if let Some(m) = config.adjust_cell_width {
                set.insert(metrics::Key::CellWidth, m);
            }
            if let Some(m) = config.adjust_cell_height {
                set.insert(metrics::Key::CellHeight, m);
            }
            if let Some(m) = config.adjust_font_baseline {
                set.insert(metrics::Key::CellBaseline, m);
            }
            if let Some(m) = config.adjust_underline_position {
                set.insert(metrics::Key::UnderlinePosition, m);
            }
            if let Some(m) = config.adjust_underline_thickness {
                set.insert(metrics::Key::UnderlineThickness, m);
            }
            if let Some(m) = config.adjust_strikethrough_position {
                set.insert(metrics::Key::StrikethroughPosition, m);
            }
            if let Some(m) = config.adjust_strikethrough_thickness {
                set.insert(metrics::Key::StrikethroughThickness, m);
            }
            if let Some(m) = config.adjust_overline_position {
                set.insert(metrics::Key::OverlinePosition, m);
            }
            if let Some(m) = config.adjust_overline_thickness {
                set.insert(metrics::Key::OverlineThickness, m);
            }
            if let Some(m) = config.adjust_cursor_thickness {
                set.insert(metrics::Key::CursorThickness, m);
            }
            if let Some(m) = config.adjust_cursor_height {
                set.insert(metrics::Key::CursorHeight, m);
            }
            if let Some(m) = config.adjust_box_thickness {
                set.insert(metrics::Key::BoxThickness, m);
            }
            if let Some(m) = config.adjust_icon_height {
                set.insert(metrics::Key::IconHeight, m);
            }
            set
        };

        // const regular_offset = config.@"font-family".list.items.len;
        let regular_offset = config.font_family.list.len();
        // const bold_offset = regular_offset + config.@"font-family-bold".list.items.len;
        let bold_offset = regular_offset + config.font_family_bold.list.len();
        let italic_offset = bold_offset + config.font_family_italic.list.len();
        let bold_italic_offset = italic_offset + config.font_family_bold_italic.list.len();

        Key {
            descriptors,
            style_offsets: [
                regular_offset,
                bold_offset,
                italic_offset,
                bold_italic_offset,
            ],
            codepoint_map,
            metric_modifiers,
            font_size,
            // .freetype_load_flags = if (FreetypeLoadFlags != void) config.@"freetype-load-flags" else font.face.freetype_load_flags_default,
            freetype_load_flags: config.freetype_load_flags,
        }
    }

    // pub fn deinit(self: *Key) void { self.arena.deinit(); }
    // PORT NOTE: owned fields free on drop; no arena.
    pub fn deinit(&mut self) {}

    /// Get the descriptors for the given font style that can be
    /// used with discovery.
    // pub fn descriptorsForStyle(self: Key, style: Style) []const discovery.Descriptor {
    pub fn descriptors_for_style(&self, style: Style) -> &[Descriptor] {
        // const idx = @intFromEnum(style);
        let idx = style as usize;
        // const start: usize = if (idx == 0) 0 else self.style_offsets[idx - 1];
        let start: usize = if idx == 0 {
            0
        } else {
            self.style_offsets[idx - 1]
        };
        // const end = self.style_offsets[idx];
        let end = self.style_offsets[idx];
        // return self.descriptors[start..end];
        &self.descriptors[start..end]
    }

    /// Hash the key with the given hasher.
    // pub fn hash(self: Key, hasher: anytype) void {
    #[allow(clippy::should_implement_trait)]
    pub fn hash<H: core::hash::Hasher>(&self, hasher: &mut H) {
        use core::hash::Hash;
        // const autoHash = std.hash.autoHash;
        // autoHash(hasher, @as(u32, @bitCast(self.font_size.points)));
        self.font_size.points.to_bits().hash(hasher);
        // autoHash(hasher, self.font_size.xdpi);
        self.font_size.xdpi.hash(hasher);
        // autoHash(hasher, self.font_size.ydpi);
        self.font_size.ydpi.hash(hasher);
        // autoHash(hasher, self.descriptors.len);
        self.descriptors.len().hash(hasher);
        // for (self.descriptors) |d| d.hash(hasher);
        for d in &self.descriptors {
            d.hash(hasher);
        }
        // self.codepoint_map.hash(hasher);
        self.codepoint_map.hash(hasher);
        // autoHash(hasher, self.metric_modifiers.count());
        self.metric_modifiers.len().hash(hasher);
        // autoHash(hasher, self.freetype_load_flags);
        //
        // PORT NOTE: on the `void`-arm backends (macOS `coretext`, wasm
        // `web_canvas`) `FreetypeLoadFlags` is `()`, whose `Hash` impl feeds the
        // hasher nothing — exactly matching `autoHash` of a `void` field. On the
        // freetype-active backends it is the 5-bool stand-in and the derived
        // `Hash` walks the fields, as `autoHash` walks a packed struct.
        //
        // PARITY: on the `void`-arm backends clippy's `unit_hash`/`unit_arg`
        // fire because hashing `()` is a no-op — which is PRECISELY the
        // reference's behaviour for `autoHash` of a `void` field. Suppressing
        // the call instead would drop the freetype-active arm, where the field
        // MUST be hashed (two configs differing only in `freetype-load-flags`
        // would otherwise collide on the same cached grid).
        #[allow(clippy::unit_hash, clippy::unit_arg)]
        self.freetype_load_flags.hash(hasher);
        // if (self.metric_modifiers.count() > 0) {
        if !self.metric_modifiers.is_empty() {
            // inline for (@typeInfo(Metrics.Key).@"enum".fields) |field| {
            for key in [
                metrics::Key::CellWidth,
                metrics::Key::CellHeight,
                metrics::Key::CellBaseline,
                metrics::Key::UnderlinePosition,
                metrics::Key::UnderlineThickness,
                metrics::Key::StrikethroughPosition,
                metrics::Key::StrikethroughThickness,
                metrics::Key::OverlinePosition,
                metrics::Key::OverlineThickness,
                metrics::Key::BoxThickness,
                metrics::Key::CursorThickness,
                metrics::Key::CursorHeight,
                metrics::Key::IconHeight,
                metrics::Key::IconHeightSingle,
                metrics::Key::FaceWidth,
                metrics::Key::FaceHeight,
                metrics::Key::FaceY,
            ] {
                // if (self.metric_modifiers.get(key)) |value| { autoHash(hasher, key); value.hash(hasher); }
                if let Some(value) = self.metric_modifiers.get(&key) {
                    key.hash(hasher);
                    value.hash(hasher);
                }
            }
        }
    }

    /// Returns a hash code that can be used to uniquely identify this
    /// action.
    // pub fn hashcode(self: Key) u64 {
    pub fn hashcode(&self) -> u64 {
        // var hasher = std.hash.Wyhash.init(0);
        //
        // PORT NOTE: the reference seeds `std.hash.Wyhash`; rustty uses the stdlib
        // `DefaultHasher` (as `discovery.Descriptor.hashcode` does). The map key is
        // an internal cache key (never serialized/compared cross-implementation), so
        // the exact algorithm is not observable; the ported `Key` tests assert only
        // non-zero / (in)equality.
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        // self.hash(&hasher);
        self.hash(&mut hasher);
        // return hasher.final();
        core::hash::Hasher::finish(&hasher)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::face::DEFAULT_DPI;

    // Helper: `.{ .points = N }` — the reference `DesiredSize` relies on the
    // `xdpi`/`ydpi` field defaults (`default_dpi`); rustty has no field defaults,
    // so we spell them out here.
    fn size_points(points: f32) -> DesiredSize {
        DesiredSize {
            points,
            xdpi: DEFAULT_DPI,
            ydpi: DEFAULT_DPI,
        }
    }

    // test "Key"
    #[test]
    fn key() {
        let alloc = Global;
        // var cfg = try Config.default(alloc); defer cfg.deinit();
        let mut cfg = Config::default_config();

        // var keycfg = try DerivedConfig.init(alloc, &cfg); defer keycfg.deinit();
        let mut keycfg = DerivedConfig::init(&cfg);

        // var k = try Key.init(alloc, &keycfg, .{ .points = 12 }); defer k.deinit();
        let mut k = Key::init(&keycfg, size_points(12.0));

        // var k2 = try Key.init(alloc, &keycfg, .{ .points = 12 }); defer k2.deinit();
        let mut k2 = Key::init(&keycfg, size_points(12.0));

        // try testing.expect(k.hashcode() > 0);
        assert!(k.hashcode() > 0);
        // try testing.expectEqual(k.hashcode(), k2.hashcode());
        assert_eq!(k.hashcode(), k2.hashcode());

        k.deinit();
        k2.deinit();
        keycfg.deinit();
        cfg.deinit();
        let _ = alloc;
    }

    // test "Key different font points"
    #[test]
    fn key_different_font_points() {
        let alloc = Global;
        let mut cfg = Config::default_config();
        let mut keycfg = DerivedConfig::init(&cfg);

        let mut k = Key::init(&keycfg, size_points(12.0));
        // var k2 = try Key.init(alloc, &keycfg, .{ .points = 16 });
        let mut k2 = Key::init(&keycfg, size_points(16.0));

        // try testing.expect(k.hashcode() != k2.hashcode());
        assert!(k.hashcode() != k2.hashcode());

        k.deinit();
        k2.deinit();
        keycfg.deinit();
        cfg.deinit();
        let _ = alloc;
    }

    // test "Key different font DPI"
    #[test]
    fn key_different_font_dpi() {
        let alloc = Global;
        let mut cfg = Config::default_config();
        let mut keycfg = DerivedConfig::init(&cfg);

        // var k = try Key.init(alloc, &keycfg, .{ .points = 12, .xdpi = 1 });
        let mut k = Key::init(
            &keycfg,
            DesiredSize {
                points: 12.0,
                xdpi: 1,
                ydpi: DEFAULT_DPI,
            },
        );
        // var k2 = try Key.init(alloc, &keycfg, .{ .points = 12, .xdpi = 2 });
        let mut k2 = Key::init(
            &keycfg,
            DesiredSize {
                points: 12.0,
                xdpi: 2,
                ydpi: DEFAULT_DPI,
            },
        );

        // try testing.expect(k.hashcode() != k2.hashcode());
        assert!(k.hashcode() != k2.hashcode());

        k.deinit();
        k2.deinit();
        keycfg.deinit();
        cfg.deinit();
        let _ = alloc;
    }

    // W9F-02 proving test (no reference analog — Zig's test allocator catches
    // leaks structurally): when `collection()` fails partway through, the
    // materialized `errdefer c.deinit(self.alloc)` (SharedGridSet.zig:175) must
    // tear down the faces already owned by the partially-built collection.
    // Discovery is skipped (`font_discover = None`) so the build reduces to the
    // embedded-font `add`s; the set-size fail-point makes the SECOND `add`
    // fail. Fails without the error-path `c.deinit()` in `collection`.
    #[test]
    fn collection_error_exit_deinits_collection() {
        let alloc = Global;
        let mut set = SharedGridSet::init(alloc).unwrap();
        let mut cfg = Config::default_config();
        let mut keycfg = DerivedConfig::init(&cfg);
        let mut key = Key::init(&keycfg, size_points(12.0));

        let deinits_before = crate::face::test_probe::face_deinits();
        // Fail the second `set_size` call: the first embedded `add` succeeds
        // (one loaded face owned by the collection), the second fails.
        crate::face::test_probe::arm_set_size_failpoint(1);
        let result = SharedGridSet::<Global>::collection(
            &mut None,
            set.font_lib,
            &key,
            size_points(12.0),
            &keycfg,
        );
        crate::face::test_probe::disarm_set_size_failpoint();
        assert!(result.is_err());

        // Expected teardown:
        //   +1 — the second `add`'s moved face (deinit in `add`'s error arm),
        //   +1 — the first entry's loaded face (the error-path `c.deinit()`).
        // The CoreText backend's `set_size` additionally deinits the face it
        // replaces on the ONE successful resize (+1, CoreText face only;
        // FreeType resizes in place — keyed on the active `Face`, so
        // `coretext_freetype` on macOS takes the FreeType count).
        #[cfg(not(rustty_font_has_freetype))]
        let expected = deinits_before + 3;
        #[cfg(rustty_font_has_freetype)]
        let expected = deinits_before + 2;
        assert_eq!(crate::face::test_probe::face_deinits(), expected);

        key.deinit();
        keycfg.deinit();
        cfg.deinit();
        set.deinit();
    }

    // test SharedGridSet
    //
    // PORT NOTE: `set.ref` resolves the configured font families through the
    // active backend discoverer (`Discover::init` + `.discover`). Every native
    // backend now has a live discoverer — the CoreText one on macOS, the real
    // Fontconfig one on Linux/BSD (`discovery.rs`'s `fontconfig_impl`: `init`/
    // `discover`/`discover_fallback`/`next`), and the directory scanner on
    // Windows — so the test is ungated on native exactly like the reference.
    // The only skip the reference has is `Discover == void`, which is the
    // `.web_canvas => void` wasm arm; that is the `not(wasm32)` gate below.
    // CORRECTION (S-14): `test SharedGridSet` (`SharedGridSet.zig:822`) has no
    // skip at all; on the `.freetype => void` native backend it runs, and
    // `discover()` returning null only logs (`DISCOVER_IS_VOID`), so it runs
    // here too. The wasm gate is this crate's own (no `SharedGridSet` on wasm).
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn shared_grid_set() {
        let alloc = Global;

        // var set = try SharedGridSet.init(alloc); defer set.deinit();
        let mut set = SharedGridSet::init(alloc).unwrap();

        // var cfg = try Config.default(alloc); defer cfg.deinit();
        let mut cfg = Config::default_config();

        // var keycfg = try DerivedConfig.init(alloc, &cfg); defer keycfg.deinit();
        let mut keycfg = DerivedConfig::init(&cfg);

        // Get a grid for the given config
        // const key1, const grid1 = try set.ref(&keycfg, .{ .points = 12 });
        let (key1, grid1) = set.r#ref(&keycfg, size_points(12.0)).unwrap();
        // try testing.expectEqual(@as(usize, 1), set.count());
        assert_eq!(1usize, set.count());

        // Get another
        // const key2, const grid2 = try set.ref(&keycfg, .{ .points = 12 });
        let (key2, grid2) = set.r#ref(&keycfg, size_points(12.0)).unwrap();
        assert_eq!(1usize, set.count());

        // They should be pointer equivalent
        // try testing.expectEqual(@intFromPtr(grid1), @intFromPtr(grid2));
        assert_eq!(grid1 as usize, grid2 as usize);

        // If I deref grid2 then we should still have a count of 1
        set.deref(key2);
        assert_eq!(1usize, set.count());

        // If I deref grid1 then we should have a count of 0
        set.deref(key1);
        assert_eq!(0usize, set.count());

        set.deinit();
        keycfg.deinit();
        cfg.deinit();
    }
}
