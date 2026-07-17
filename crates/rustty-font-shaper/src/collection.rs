//! A font collection is a list of faces of different styles. The list is
//! ordered by priority (per style). All fonts in a collection share the same
//! size so they can be used interchangeably in cases a glyph is missing in one
//! and present in another.
//!
//! The purpose of a collection is to store a list of fonts by style
//! and priority order. A collection does not handle searching for font
//! callbacks, rasterization, etc. For this, see CodepointResolver.
//!
//! The collection can contain both loaded and deferred faces. Deferred faces
//! typically use less memory while still providing some necessary information
//! such as codepoint support, presentation, etc. This is useful for looking
//! for fallback fonts as efficiently as possible. For example, when the glyph
//! "X" is not found, we can quickly search through deferred fonts rather
//! than loading the font completely.
// const Collection = @This();

// const std = @import("std");
// const Allocator = std.mem.Allocator;
// const config = @import("../config.zig");
// const comparison = @import("../datastruct/comparison.zig");
// const font = @import("main.zig");
// const options = font.options;
// const DeferredFace = font.DeferredFace;
// const DesiredSize = font.face.DesiredSize;
// const Face = font.Face;
// const Library = font.Library;
// const Metrics = font.Metrics;
// const Presentation = font.Presentation;
// const Style = font.Style;
//
// PORT NOTE (imports): `config.FontSyntheticStyle` (used by `completeStyles`)
// lives in the higher `rustty-config` crate (break #9: config injected via DI),
// so it is NOT a dependency of this tier — it is modeled locally by
// `FontSyntheticStyle` below (a faithful stand-in for the three bool fields
// `completeStyles` reads). `comparison` is `rustty_datastruct::comparison`,
// `options` is `crate::options`, and the `font.*` types map to the `main.zig`
// glue re-exports in `lib.rs`.

use crate::deferred_face::DeferredFace;
use crate::face::{DesiredSize, Face, FaceError, Options};
use crate::library::Library;
use crate::{Presentation, Style};
use rustty_font::metrics::{FaceMetrics, Metrics, ModifierSet};

// const log = std.log.scoped(.font_collection);

/// The metric modifiers to use for this collection. The memory
/// for this is owned by the user and is not freed by the collection.
///
/// (See the `Collection` struct fields below; the reference declares them as
/// bare file-struct fields on `const Collection = @This()`.)
pub struct Collection {
    /// The available faces we have. This shouldn't be modified manually.
    /// Instead, use the functions available on Collection.
    faces: StyleArray,

    /// The metric modifiers to use for this collection. The memory
    /// for this is owned by the user and is not freed by the collection.
    ///
    /// Call `Collection.updateMetrics` to recompute the
    /// collection's metrics after making changes to these.
    // metric_modifiers: Metrics.ModifierSet = .{},
    pub metric_modifiers: ModifierSet,

    /// Metrics for this collection. Call `Collection.updateMetrics` to (re)compute
    /// these after adding a primary font or making changes to `metric_modifiers`.
    // metrics: ?Metrics = null,
    pub metrics: Option<Metrics>,

    /// The face metrics for the primary face in the collection.
    ///
    /// We keep this around so we don't need to re-compute it when calculating
    /// the scale factor for additional fonts added to the collection.
    // primary_face_metrics: ?Metrics.FaceMetrics = null,
    pub primary_face_metrics: Option<FaceMetrics>,

    /// The load options for deferred faces in the face list. If this
    /// is not set, then deferred faces will not be loaded. Attempting to
    /// add a deferred face will result in an error.
    // load_options: ?LoadOptions = null,
    pub load_options: Option<LoadOptions>,
}

/// PORT NOTE: the reference's error sets (`AddError`, `AddDeferredError`,
/// `EntryError`, `CompleteError`, `SetSizeError`, `UpdateMetricsError`, and the
/// inferred error unions of `getFace`/`getFaceFromEntry`/`syntheticBold`/
/// `syntheticItalic`) are consolidated into one enum with `From` conversions so
/// `?` composes them, mirroring the coretext face's single-enum approach. The
/// specific variants the tests assert (`CollectionFull`,
/// `DeferredLoadingUnavailable`) are preserved.
#[derive(Debug)]
pub enum Error {
    /// `Allocator.Error` — reproduced for call-shape fidelity (allocations here
    /// abort on OOM rather than returning, but the variant is kept for the
    /// reference error unions).
    OutOfMemory,
    /// There's no more room in the collection.
    CollectionFull,
    /// The call to `face.setSize` failed.
    SetSizeFailed,
    /// `load_options` is null, can't do deferred loading.
    DeferredLoadingUnavailable,
    /// Index represents a special font (built-in) and these don't
    /// have an associated face. This should be caught upstream and use
    /// alternate logic.
    SpecialHasNoFace,
    /// Invalid index.
    IndexOutOfBounds,
    /// `completeStyles` couldn't find a default regular text face.
    DefaultUnavailable,
    /// `updateMetrics` couldn't load the primary font.
    CannotLoadPrimaryFont,
    /// The backend didn't support the requested synthetic style.
    SyntheticBoldUnavailable,
    SyntheticItalicUnavailable,
    /// A backend `Face` operation (load/setSize/init) failed. Wraps the
    /// backend-face error (`coretext::Error` / `freetype::Error`).
    Face(FaceError),
}

impl From<FaceError> for Error {
    fn from(e: FaceError) -> Self {
        Error::Face(e)
    }
}

/// Faithful stand-in for `config.FontSyntheticStyle` (break #9: `rustty-config`
/// is not a dep of this tier). The reference type has these three bool fields,
/// read by `completeStyles`.
// pub const FontSyntheticStyle = struct { bold, italic, @"bold-italic" };
#[derive(Clone, Copy, Debug)]
pub struct FontSyntheticStyle {
    pub bold: bool,
    pub italic: bool,
    pub bold_italic: bool,
}

impl Default for FontSyntheticStyle {
    // The reference defaults every synthetic style to enabled (`= true`).
    fn default() -> Self {
        FontSyntheticStyle {
            bold: true,
            italic: true,
            bold_italic: true,
        }
    }
}

impl Collection {
    /// Initialize an empty collection.
    // pub fn init() Collection {
    pub fn init() -> Collection {
        // Initialize our styles array, preallocating some space that is
        // likely to be used.
        // return .{ .faces = .initFill(.{}) };
        Collection {
            faces: StyleArray::init_fill(),
            metric_modifiers: ModifierSet::new(),
            metrics: None,
            primary_face_metrics: None,
            load_options: None,
        }
    }

    // pub fn deinit(self: *Collection, alloc: Allocator) void {
    pub fn deinit(&mut self) {
        let mut it = self.faces.iterator();
        while let Some(array) = it.next() {
            let list = &mut self.faces.0[array];
            // Deinit all entries, aliases can be ignored.
            for entry_or_alias in list.iter_mut() {
                match entry_or_alias.as_mut() {
                    EntryOrAlias::Entry(entry) => entry.deinit(),
                    EntryOrAlias::Alias(_) => {}
                }
            }
            // array.value.deinit(alloc); — the `Vec<Box<..>>` frees on `clear`.
            list.clear();
        }

        // if (self.load_options) |*v| v.deinit(alloc);
        if let Some(v) = self.load_options.as_mut() {
            v.deinit();
        }
    }

    /// Add a face to the collection. This face will be added next in priority if
    /// others exist already, i.e. it'll be the _last_ to be searched for a glyph
    /// in that list.
    ///
    /// If no error is encountered then the collection takes ownership of the face,
    /// in which case face will be deallocated when the collection is deallocated.
    ///
    /// When added, the size of the face will be adjusted to match `load_options`.
    ///
    /// Returns the index for the added face.
    // pub fn add(self, alloc, face: Face, opts: AddOptions) AddError!Index {
    pub fn add(&mut self, face: Face, opts: AddOptions) -> Result<Index, Error> {
        // const list = self.faces.getPtr(opts.style);
        // We have some special indexes so we must never pass those.
        let idx = self.faces.get(opts.style).count();
        if idx >= (Index::SPECIAL_START as usize) - 1 {
            return Err(Error::CollectionFull);
        }

        let mut owned_face = face;

        // Scale factor to adjust the size of the added face.
        let scale_factor = self.scale_factor(owned_face.get_metrics(), opts.size_adjustment);

        // If we have load options, we update the size to ensure
        // it's matches and is normalized to the primary if possible.
        if let Some(load_opts) = self.load_options {
            let mut new_opts = load_opts;
            new_opts.size.points *= scale_factor as f32;
            owned_face
                .set_size(new_opts.face_options())
                .map_err(|_| Error::SetSizeFailed)?;
        }

        // try list.append(alloc, .{ .entry = .{ ... } });
        self.faces
            .get_mut(opts.style)
            .push(Box::new(EntryOrAlias::Entry(Entry {
                face: AnyFace::Loaded(owned_face),
                fallback: opts.fallback,
                scale_factor: ScaleFactor::Scale(scale_factor),
            })));

        // return .{ .style = opts.style, .idx = @intCast(idx) };
        Ok(Index::new(opts.style, idx as u16))
    }

    /// Add a deferred face to the collection.
    ///
    /// Returns the index for the added face.
    // pub fn addDeferred(self, alloc, face: DeferredFace, opts) AddDeferredError!Index {
    pub fn add_deferred(&mut self, face: DeferredFace, opts: AddOptions) -> Result<Index, Error> {
        if self.load_options.is_none() {
            return Err(Error::DeferredLoadingUnavailable);
        }

        // We have some special indexes so we must never pass those.
        let idx = self.faces.get(opts.style).count();
        if idx >= (Index::SPECIAL_START as usize) - 1 {
            return Err(Error::CollectionFull);
        }

        self.faces
            .get_mut(opts.style)
            .push(Box::new(EntryOrAlias::Entry(Entry {
                face: AnyFace::Deferred(face),
                fallback: opts.fallback,
                scale_factor: ScaleFactor::Adjustment(opts.size_adjustment),
            })));

        Ok(Index::new(opts.style, idx as u16))
    }

    /// Return the Face represented by a given Index. The returned pointer
    /// is only valid as long as this collection is not modified.
    ///
    /// This will initialize the face if it is deferred and not yet loaded,
    /// which can fail.
    // pub fn getFace(self, index) !*Face {
    pub fn get_face(&mut self, index: Index) -> Result<*mut Face, Error> {
        let entry = self.get_entry(index)?;
        self.get_face_from_entry(entry)
    }

    /// Number of faces registered for a given style.
    //
    // PORT NOTE (glue accessor): the reference `discovery.zig`'s `discoverCodepoint`
    // reads `collection.faces.get(style).count()` directly (the `font` module has
    // access to the file-struct `faces` field). rustty keeps `faces` private, so
    // this crate-visible accessor exposes exactly that count for the `discovery`
    // sibling module. No reference logic is added.
    // NON-MACOS: the only caller is the CoreText discoverer (`discovery.rs`,
    // macOS-gated), so this is unused on every other backend — silence dead-code
    // there. This covers the wasm web_canvas backend (whose noop discoverer never
    // calls it) and the Linux fontconfig_freetype backend (whose discoverer is an
    // `unimplemented!()` stub until L3 Task 6/7; note the reference's
    // `Fontconfig.discoverFallback` discards `collection` entirely, so the real
    // Fontconfig discoverer will not call this either).
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub(crate) fn style_face_count(&self, style: Style) -> usize {
        self.faces.get(style).count()
    }

    /// Get the unaliased entry from an index
    // pub fn getEntry(self, index) EntryError!*Entry {
    pub fn get_entry(&mut self, index: Index) -> Result<*mut Entry, Error> {
        if index.special().is_some() {
            return Err(Error::SpecialHasNoFace);
        }
        let list = self.faces.get_mut(index.style());
        if (index.idx() as usize) >= list.len() {
            return Err(Error::IndexOutOfBounds);
        }
        // return list.at(index.idx).getEntry();
        Ok(list[index.idx() as usize].get_entry())
    }

    /// Get the face from an entry.
    ///
    /// This entry must not be an alias.
    // fn getFaceFromEntry(self, entry: *Entry) !*Face {
    fn get_face_from_entry(&mut self, entry: *mut Entry) -> Result<*mut Face, Error> {
        // SAFETY: `entry` points into a `Box` owned by `self.faces`; it is stable
        // as long as the collection is not modified (the reference's contract).
        let entry_ref = unsafe { &mut *entry };
        match &mut entry_ref.face {
            AnyFace::Deferred(d) => {
                // var opts = self.load_options orelse
                //     return error.DeferredLoadingUnavailable;
                let mut opts = match self.load_options {
                    Some(o) => o,
                    None => return Err(Error::DeferredLoadingUnavailable),
                };

                // Load the face.
                let mut face = d.load(opts.library, opts.face_options())?;
                // errdefer face.deinit();

                // Calculate the scale factor for this
                // entry now that we have a loaded face.
                if let ScaleFactor::Adjustment(adjustment) = entry_ref.scale_factor {
                    let factor = self.scale_factor(face.get_metrics(), adjustment);
                    entry_ref.scale_factor = ScaleFactor::Scale(factor);
                }

                // If our scale factor is something other
                // than 1.0 then we need to resize the face.
                if let ScaleFactor::Scale(scale) = entry_ref.scale_factor {
                    if scale != 1.0 {
                        opts.size.points *= scale as f32;
                        face.set_size(opts.face_options())?;
                    }
                }

                // Deinit the deferred face now that we have
                // loaded it and are past any possible errors.
                // errdefer comptime unreachable;
                d.deinit();

                // Set the loaded face on the entry.
                entry_ref.face = AnyFace::Loaded(face);

                // Return the pointer to it.
                match &mut entry_ref.face {
                    AnyFace::Loaded(f) => Ok(f as *mut Face),
                    AnyFace::Deferred(_) => unreachable!(),
                }
            }

            AnyFace::Loaded(f) => Ok(f as *mut Face),
        }
    }

    /// Return the index of the font in this collection that contains
    /// the given codepoint, style, and presentation. If no font is found,
    /// null is returned.
    ///
    /// This does not trigger font loading; deferred fonts can be
    /// searched for codepoints.
    // pub fn getIndex(self: *const Collection, cp, style, p_mode) ?Index {
    pub fn get_index(&self, cp: u32, style: Style, p_mode: PresentationMode) -> Option<Index> {
        // var i: usize = 0; ... i += 1;  (a manual counter in the reference,
        // expressed as `enumerate()` here.)
        let list = self.faces.get(style);
        for (i, entry_or_alias) in list.iter().enumerate() {
            // SAFETY: `get_const_entry` returns a pointer into a stable `Box`.
            let entry = unsafe { &*entry_or_alias.get_const_entry() };
            if entry.has_codepoint(cp, p_mode) {
                return Some(Index::new(style, i as u16));
            }
        }

        // Not found
        None
    }

    /// Check if a specific font index has a specific codepoint. This does not
    /// necessarily force the font to load. The presentation value "p" will
    /// verify the Emoji representation matches if it is non-null. If "p" is
    /// null then any presentation will be accepted.
    // pub fn hasCodepoint(self: *const Collection, index, cp, p_mode) bool {
    pub fn has_codepoint(&self, index: Index, cp: u32, p_mode: PresentationMode) -> bool {
        let list = self.faces.get(index.style());
        if (index.idx() as usize) >= list.count() {
            return false;
        }
        // return list.at(index.idx).getConstEntry().hasCodepoint(cp, p_mode);
        let entry = unsafe { &*list[index.idx() as usize].get_const_entry() };
        entry.has_codepoint(cp, p_mode)
    }

    /// Ensure we have an option for all styles in the collection, such
    /// as italic and bold by synthesizing them if necessary from the
    /// first regular face that has text glyphs.
    ///
    /// If there is no regular face that has text glyphs, then this
    /// does nothing.
    // pub fn completeStyles(self, alloc, synthetic_config) CompleteError!void {
    pub fn complete_styles(&mut self, synthetic_config: FontSyntheticStyle) -> Result<(), Error> {
        // If every style has at least one entry then we're done!
        // This is the most common case.
        'empty: {
            let mut all_nonempty = true;
            let mut it = self.faces.iterator();
            while let Some(idx) = it.next() {
                if self.faces.0[idx].count() == 0 {
                    all_nonempty = false;
                    break;
                }
            }
            if !all_nonempty {
                break 'empty;
            }

            return Ok(());
        }

        // Find the first regular face that has non-colorized text glyphs.
        // This is the font we want to fallback to. This may not be index zero
        // if a user configures something like an Emoji font first.
        // const regular_entry: *Entry = entry: { ... };
        let regular_entry: *mut Entry = 'entry: {
            let list_len = self.faces.get(Style::Regular).count();
            if list_len == 0 {
                return Ok(());
            }

            // Find our first regular face that has text glyphs.
            let mut index: usize = 0;
            while index < self.faces.get(Style::Regular).count() {
                // Load our face. If we fail to load it, we just skip it and
                // continue on to try the next one.
                let entry = self.faces.get_mut(Style::Regular)[index].get_entry();
                let face = match self.get_face_from_entry(entry) {
                    Ok(f) => f,
                    Err(_err) => {
                        // log.warn("error loading regular entry={d} err={}", .{ it.index - 1, err });
                        index += 1;
                        continue;
                    }
                };

                // We have two conditionals here. The color check is obvious:
                // we want to auto-italicize a normal text font. The second
                // check is less obvious... for mixed color/non-color fonts, we
                // accept the regular font if it has basic ASCII. This may not
                // be strictly correct (especially with international fonts) but
                // it's a reasonable heuristic and the first case will match 99%
                // of the time.
                let face_ref = unsafe { &*face };
                if !face_ref.has_color() || face_ref.glyph_index(u32::from('A')).is_some() {
                    break 'entry entry;
                }

                index += 1;
            }

            // No regular text face found. We can't provide any fallback.
            return Err(Error::DefaultUnavailable);
        };

        // If we don't have italic, attempt to create a synthetic italic face.
        // If we can't create a synthetic italic face, we'll just use the regular
        // face for italic.
        let have_italic = self.faces.get(Style::Italic).count() > 0;
        if !have_italic {
            'italic: {
                if !synthetic_config.italic {
                    // log.info("italic style not available and synthetic italic disabled", .{});
                    self.faces
                        .get_mut(Style::Italic)
                        .push(Box::new(EntryOrAlias::Alias(regular_entry)));
                    break 'italic;
                }

                let synthetic = match self.synthetic_italic(regular_entry) {
                    Ok(s) => s,
                    Err(_err) => {
                        // log.warn("failed to create synthetic italic, italic style will not be available err={}", .{err});
                        self.faces
                            .get_mut(Style::Italic)
                            .push(Box::new(EntryOrAlias::Alias(regular_entry)));
                        break 'italic;
                    }
                };

                let synthetic_entry = Entry {
                    face: AnyFace::Loaded(synthetic),
                    fallback: false,
                    // (reference: scale_factor default `.{ .scale = 1.0 }`)
                    scale_factor: ScaleFactor::Scale(1.0),
                };
                // log.info("synthetic italic face created", .{});
                self.faces
                    .get_mut(Style::Italic)
                    .push(Box::new(EntryOrAlias::Entry(synthetic_entry)));
            }
        }

        // If we don't have bold, use the regular font.
        let have_bold = self.faces.get(Style::Bold).count() > 0;
        if !have_bold {
            'bold: {
                if !synthetic_config.bold {
                    // log.info("bold style not available and synthetic bold disabled", .{});
                    self.faces
                        .get_mut(Style::Bold)
                        .push(Box::new(EntryOrAlias::Alias(regular_entry)));
                    break 'bold;
                }

                let synthetic = match self.synthetic_bold(regular_entry) {
                    Ok(s) => s,
                    Err(_err) => {
                        // log.warn("failed to create synthetic bold, bold style will not be available err={}", .{err});
                        self.faces
                            .get_mut(Style::Bold)
                            .push(Box::new(EntryOrAlias::Alias(regular_entry)));
                        break 'bold;
                    }
                };

                let synthetic_entry = Entry {
                    face: AnyFace::Loaded(synthetic),
                    fallback: false,
                    scale_factor: ScaleFactor::Scale(1.0),
                };
                // log.info("synthetic bold face created", .{});
                self.faces
                    .get_mut(Style::Bold)
                    .push(Box::new(EntryOrAlias::Entry(synthetic_entry)));
            }
        }

        // If we don't have bold italic, we attempt to synthesize a bold variant
        // of the italic font. If we can't do that, we'll use the italic font.
        if self.faces.get(Style::BoldItalic).count() == 0 {
            'bold_italic: {
                if !synthetic_config.bold_italic {
                    // log.info("bold italic style not available and synthetic bold italic disabled", .{});
                    self.faces
                        .get_mut(Style::BoldItalic)
                        .push(Box::new(EntryOrAlias::Alias(regular_entry)));
                    break 'bold_italic;
                }

                // Prefer to synthesize on top of the face we already had. If we
                // have bold then we try to synthesize italic on top of bold.
                if have_bold {
                    let base_entry: *mut Entry = self.faces.get_mut(Style::Bold)[0].get_entry();
                    if let Ok(synthetic) = self.synthetic_italic(base_entry) {
                        // log.info("synthetic bold italic face created from bold", .{});
                        let synthetic_entry = Entry {
                            face: AnyFace::Loaded(synthetic),
                            fallback: false,
                            scale_factor: ScaleFactor::Scale(1.0),
                        };
                        self.faces
                            .get_mut(Style::BoldItalic)
                            .push(Box::new(EntryOrAlias::Entry(synthetic_entry)));
                        break 'bold_italic;
                    }

                    // If synthesizing italic failed, then we try to synthesize
                    // bold on whatever italic font we have.
                }

                let base_entry: *mut Entry = self.faces.get_mut(Style::Italic)[0].get_entry();
                if let Ok(synthetic) = self.synthetic_bold(base_entry) {
                    // log.info("synthetic bold italic face created from italic", .{});
                    let synthetic_entry = Entry {
                        face: AnyFace::Loaded(synthetic),
                        fallback: false,
                        scale_factor: ScaleFactor::Scale(1.0),
                    };
                    self.faces
                        .get_mut(Style::BoldItalic)
                        .push(Box::new(EntryOrAlias::Entry(synthetic_entry)));
                    break 'bold_italic;
                }

                // log.warn("bold italic style not available, using italic font", .{});
                self.faces
                    .get_mut(Style::BoldItalic)
                    .push(Box::new(EntryOrAlias::Alias(base_entry)));
            }
        }

        Ok(())
    }

    /// Create a synthetic bold font face from the given entry and return it.
    // fn syntheticBold(self, entry: *Entry) !Face {
    fn synthetic_bold(&mut self, entry: *mut Entry) -> Result<Face, Error> {
        // Not all font backends support synthetic bold.
        // if (comptime !@hasDecl(Face, "syntheticBold")) return error.SyntheticBoldUnavailable;
        //
        // PORT NOTE: both ported backend faces (`coretext`/`freetype`) declare
        // `synthetic_bold`, so `@hasDecl` is comptime-true and this guard never
        // triggers; kept as a comment (and `SyntheticBoldUnavailable` remains a
        // variant for the backends that omit it).

        // We require loading options to create a synthetic bold face.
        let opts = match self.load_options {
            Some(o) => o,
            None => return Err(Error::DeferredLoadingUnavailable),
        };

        // Try to bold it.
        let regular = self.get_face_from_entry(entry)?;
        let regular = unsafe { &*regular };

        // Inherit size from regular; it may be different than opts.size
        // due to scaling adjustments
        let mut face_opts = opts.face_options();
        face_opts.size = regular.size;
        let face = regular.synthetic_bold(opts.face_options())?;
        // PORT NOTE: the reference computes `face_opts` (with the inherited size)
        // but then passes `opts.faceOptions()` to `syntheticBold`; `face_opts` is
        // dead. Preserved faithfully (the assignment is a no-op).
        let _ = face_opts;

        let mut buf = [0u8; 256];
        // if (face.name(&buf)) |name| { log.info("font synthetic bold created family={s}", .{name}); } else |_| {}
        let _ = face.name(&mut buf);

        Ok(face)
    }

    /// Create a synthetic italic font face from the given entry and return it.
    // fn syntheticItalic(self, entry: *Entry) !Face {
    fn synthetic_italic(&mut self, entry: *mut Entry) -> Result<Face, Error> {
        // Not all font backends support synthetic italicization.
        // if (comptime !@hasDecl(Face, "syntheticItalic")) return error.SyntheticItalicUnavailable;

        // We require loading options to create a synthetic italic face.
        let opts = match self.load_options {
            Some(o) => o,
            None => return Err(Error::DeferredLoadingUnavailable),
        };

        // Try to italicize it.
        let regular = self.get_face_from_entry(entry)?;
        let regular = unsafe { &*regular };

        // Inherit size from regular; it may be different than opts.size
        // due to scaling adjustments
        let mut face_opts = opts.face_options();
        face_opts.size = regular.size;
        let face = regular.synthetic_italic(opts.face_options())?;
        // PORT NOTE: `face_opts` is dead in the reference too (see `syntheticBold`).
        let _ = face_opts;

        let mut buf = [0u8; 256];
        // if (face.name(&buf)) |name| { log.info("font synthetic italic created family={s}", .{name}); } else |_| {}
        let _ = face.name(&mut buf);

        Ok(face)
    }

    /// Update the size of all faces in the collection. This will
    /// also update the size in the load options for future deferred
    /// face loading.
    ///
    /// This requires load options to be set.
    // pub fn setSize(self, size: DesiredSize) SetSizeError!void {
    pub fn set_size(&mut self, size: DesiredSize) -> Result<(), Error> {
        // Get a pointer to our options so we can modify the size.
        // const opts = &(self.load_options orelse return error.DeferredLoadingUnavailable);
        if self.load_options.is_none() {
            return Err(Error::DeferredLoadingUnavailable);
        }
        self.load_options.as_mut().unwrap().size = size;
        let opts = self.load_options.unwrap();

        // const primary_entry = self.getEntry(.{ .idx = 0 }) catch null;
        let primary_entry: *const Entry = match self.get_entry(Index::new(Style::Regular, 0)) {
            Ok(e) => e as *const Entry,
            Err(_) => core::ptr::null(),
        };

        // Resize all our faces that are loaded
        let mut it = self.faces.iterator();
        while let Some(array) = it.next() {
            let list_len = self.faces.0[array].len();
            let mut i = 0;
            while i < list_len {
                // Resize all faces. We skip entries that are aliases, since
                // the underlying face will have a non-alias entry somewhere.
                if matches!(self.faces.0[array][i].as_ref(), EntryOrAlias::Alias(_)) {
                    i += 1;
                    continue;
                }

                let entry: *mut Entry = self.faces.0[array][i].get_entry();
                let entry_ref = unsafe { &mut *entry };

                if let Some(face) = entry_ref.get_loaded() {
                    // If this isn't our primary face, we scale
                    // the size appropriately before setting it.
                    //
                    // If we don't have a primary face we also don't.
                    let mut new_opts = opts;
                    if !primary_entry.is_null()
                        && !std::ptr::eq(entry as *const Entry, primary_entry)
                    {
                        if let ScaleFactor::Scale(scale) = entry_ref.scale_factor {
                            new_opts.size.points *= scale as f32;
                        }
                    }
                    unsafe { &mut *face }
                        .set_size(new_opts.face_options())
                        .map_err(|_| Error::SetSizeFailed)?;
                }

                i += 1;
            }
        }

        self.update_metrics()
    }

    /// Calculate a factor by which to scale the provided face to match
    /// it with the primary face, depending on the specified adjustment.
    ///
    /// If this encounters any problems loading the primary face or its
    /// metrics then it just returns `1.0`.
    ///
    /// This functions very much like the `font-size-adjust` CSS property.
    /// ref: https://developer.mozilla.org/en-US/docs/Web/CSS/font-size-adjust
    // fn scaleFactor(self, face_metrics: Metrics.FaceMetrics, adjustment: SizeAdjustment) f64 {
    fn scale_factor(&mut self, face_metrics: FaceMetrics, adjustment: SizeAdjustment) -> f64 {
        // If there's no adjustment, the scale is 1.0
        if adjustment == SizeAdjustment::None {
            return 1.0;
        }

        // If we haven't calculated our primary face metrics yet, do so now.
        if self.primary_face_metrics.is_none() {
            // @branchHint(.unlikely);
            // If we can't load the primary face, just use 1.0 as the scale factor.
            let primary_face = match self.get_face(Index::new(Style::Regular, 0)) {
                Ok(f) => f,
                Err(_) => return 1.0,
            };
            self.primary_face_metrics = Some(unsafe { &*primary_face }.get_metrics());
        }

        let primary_metrics = self.primary_face_metrics.unwrap();

        // We normalize the metrics values which are expressed in px to instead
        // be in ems, so that it doesn't matter what size the faces actually are.
        let primary_scale = 1.0 / primary_metrics.px_per_em;
        let face_scale = 1.0 / face_metrics.px_per_em;

        // We get the target metrics from the primary face and this face depending
        // on the specified `adjustment`. If this face doesn't explicitly define a
        // metric, or if the value it defines is invalid, we fall through to other
        // options in the order below.
        //
        // In order to make sure the value is valid, we compare it with the result
        // of the estimator function, which rules out both null and invalid values.
        // const primary_metric: f64, const face_metric: f64 =
        //     normalize_by: switch (adjustment) { ... continue :normalize_by ... };
        //
        // PORT NOTE: the reference uses a labeled `switch` with `continue` as a
        // fall-through state machine (ic_width → ex_height → cap_height →
        // line_height). Modeled with a `loop` over a mutable `adj`.
        let (primary_metric, face_metric): (f64, f64) = {
            let mut adj = adjustment;
            loop {
                match adj {
                    SizeAdjustment::IcWidth => {
                        if face_metrics.ic_width != Some(face_metrics.ic_width()) {
                            adj = SizeAdjustment::ExHeight;
                            continue;
                        }
                        break (
                            primary_metrics.ic_width() * primary_scale,
                            face_metrics.ic_width() * face_scale,
                        );
                    }

                    SizeAdjustment::ExHeight => {
                        if face_metrics.ex_height != Some(face_metrics.ex_height()) {
                            adj = SizeAdjustment::CapHeight;
                            continue;
                        }
                        break (
                            primary_metrics.ex_height() * primary_scale,
                            face_metrics.ex_height() * face_scale,
                        );
                    }

                    SizeAdjustment::CapHeight => {
                        if face_metrics.cap_height != Some(face_metrics.cap_height()) {
                            adj = SizeAdjustment::LineHeight;
                            continue;
                        }
                        break (
                            primary_metrics.cap_height() * primary_scale,
                            face_metrics.cap_height() * face_scale,
                        );
                    }

                    SizeAdjustment::LineHeight => {
                        break (
                            primary_metrics.line_height() * primary_scale,
                            face_metrics.line_height() * face_scale,
                        );
                    }

                    SizeAdjustment::None => unreachable!(),
                }
            }
        };

        primary_metric / face_metric
    }

    /// Update the cell metrics for this collection, based on
    /// the primary font and the modifiers in `metric_modifiers`.
    ///
    /// This requires a primary font (index `0`) to be present.
    // pub fn updateMetrics(self) UpdateMetricsError!void {
    pub fn update_metrics(&mut self) -> Result<(), Error> {
        let primary_face = self
            .get_face(Index::new(Style::Regular, 0))
            .map_err(|_| Error::CannotLoadPrimaryFont)?;

        self.primary_face_metrics = Some(unsafe { &*primary_face }.get_metrics());

        let mut metrics = Metrics::calc(self.primary_face_metrics.unwrap());

        metrics.apply(&self.metric_modifiers);

        self.metrics = Some(metrics);
        Ok(())
    }
}

/// Options for adding a face to the collection.
// pub const AddOptions = struct { style, size_adjustment, fallback };
#[derive(Clone, Copy, Debug)]
pub struct AddOptions {
    /// What style this face is.
    pub style: Style,
    /// What size adjustment to use.
    pub size_adjustment: SizeAdjustment,
    /// Whether this is a fallback face.
    pub fallback: bool,
}

/// Packed array of all Style enum cases mapped to a growable list of faces.
///
/// We use this data structure because there aren't many styles and all
/// styles are typically loaded for a terminal session. The overhead per
/// style even if it is not used or barely used is minimal given the
/// small style count.
///
/// We use a segmented list because the entry values must be pointer-stable
/// to support aliases.
///
/// WARNING: We cannot use any prealloc yet for the segmented list because
/// the collection is copied around by value and pointers aren't stable.
// const StyleArray = std.EnumArray(Style, std.SegmentedList(EntryOrAlias, 0));
//
// PORT NOTE: `EnumArray(Style, ...)` is a fixed 4-element array indexed by the
// `Style` enum. `SegmentedList(EntryOrAlias, 0)` is a growable, pointer-stable
// list; modeled as `Vec<Box<EntryOrAlias>>` — each element is heap-boxed so its
// address is stable across list growth (the `alias: *Entry` pointers require it),
// reproducing the SegmentedList's pointer-stability guarantee.
//
// NOTE: `clippy::vec_box` flags the `Box` as "already on the heap", but it is
// load-bearing here — a bare `Vec<EntryOrAlias>` moves its elements on realloc,
// invalidating the raw `alias: *mut Entry` pointers; the `Box` keeps each entry
// address stable (the SegmentedList pointer-stability guarantee).
#[allow(clippy::vec_box)]
type FaceList = Vec<Box<EntryOrAlias>>;

struct StyleArray([FaceList; 4]);

impl StyleArray {
    // `.initFill(.{})` — every style starts with an empty list.
    fn init_fill() -> StyleArray {
        StyleArray([Vec::new(), Vec::new(), Vec::new(), Vec::new()])
    }

    // `self.faces.get(style)` — const list for a style.
    fn get(&self, style: Style) -> &FaceList {
        &self.0[style as usize]
    }

    // `self.faces.getPtr(style)` — mutable list for a style.
    fn get_mut(&mut self, style: Style) -> &mut FaceList {
        &mut self.0[style as usize]
    }

    // `self.faces.iterator()` — iterate styles (as array indices). The iterator
    // is index-based so it doesn't borrow the array, matching the reference's
    // `while (it.next()) |array|` where `array.value` is re-fetched per step.
    fn iterator(&self) -> StyleArrayIterator {
        StyleArrayIterator { i: 0 }
    }
}

struct StyleArrayIterator {
    i: usize,
}

impl StyleArrayIterator {
    // Returns the next style's array index, or `None` when exhausted. The
    // iterator holds no borrow of the array, so the caller re-borrows the list
    // per step (as the reference does with `array.value`), which is how the
    // borrow checker permits mutating the array between iterations.
    #[allow(clippy::should_implement_trait)]
    fn next(&mut self) -> Option<usize> {
        if self.i >= 4 {
            return None;
        }
        let idx = self.i;
        self.i += 1;
        Some(idx)
    }
}

// SegmentedList helpers used above: `count()`/`len` on a `Vec<Box<_>>` list.
trait SegmentedListExt {
    fn count(&self) -> usize;
}
impl SegmentedListExt for FaceList {
    // `list.count()` — the reference's element count.
    fn count(&self) -> usize {
        self.len()
    }
}

/// Load options are used to configure all the details a Collection
/// needs to load deferred faces.
// pub const LoadOptions = struct { library, size, freetype_load_flags };
#[derive(Clone, Copy, Debug)]
pub struct LoadOptions {
    /// The library to use for loading faces. This is not owned by
    /// the collection and can be used by multiple collections. When
    /// deinitializing the collection, the library is not deinitialized.
    pub library: Library,

    /// The desired font size for all loaded faces.
    // size: DesiredSize = .{ .points = 12 },
    pub size: DesiredSize,

    /// Freetype Load Flags to use when loading glyphs. This is a list of
    /// bitfield constants that controls operations to perform during glyph
    /// loading. Only a subset is exposed for configuration, for the whole set
    /// of flags see `pkg.freetype.face.LoadFlags`.
    // freetype_load_flags: font.face.FreetypeLoadFlags = font.face.freetype_load_flags_default,
    pub freetype_load_flags: crate::face::FreetypeLoadFlags,
}

impl LoadOptions {
    // pub fn deinit(self: *LoadOptions, alloc: Allocator) void { _ = self; _ = alloc; }
    pub fn deinit(&mut self) {}

    /// The options to use for loading faces.
    // pub fn faceOptions(self: *const LoadOptions) font.face.Options {
    pub fn face_options(&self) -> Options {
        Options {
            size: self.size,
            freetype_load_flags: self.freetype_load_flags,
        }
    }
}

impl Default for LoadOptions {
    // PORT NOTE: the reference's field defaults (`size = .{ .points = 12 }`,
    // `freetype_load_flags = freetype_load_flags_default`) are applied here so a
    // `LoadOptions { library, ..Default::default() }`-style construction matches
    // the Zig `.{ .library = lib }` (which relies on those defaults). `library`
    // has no default; callers always set it.
    fn default() -> Self {
        LoadOptions {
            // A placeholder library; overwritten by every constructor. Never used
            // because callers always provide `library` explicitly.
            library: Library::init().unwrap(),
            size: DesiredSize {
                points: 12.0,
                xdpi: crate::face::DEFAULT_DPI,
                ydpi: crate::face::DEFAULT_DPI,
            },
            freetype_load_flags: crate::face::FREETYPE_LOAD_FLAGS_DEFAULT,
        }
    }
}

/// A entry in a collection can be deferred or loaded. A deferred face
/// is not yet fully loaded and only represents the font descriptor
/// and usually uses less resources. A loaded face is fully parsed,
/// ready to rasterize, and usually uses more resources than a
/// deferred version.
///
/// A face can also be a "fallback" variant that is still either
/// deferred or loaded. Today, there is only one difference between
/// fallback and non-fallback (or "explicit") faces: the handling
/// of emoji presentation.
///
/// For explicit faces, when an explicit emoji presentation is
/// not requested, we will use any glyph for that codepoint found
/// even if the font presentation does not match the UCD
/// (Unicode Character Database) value. When an explicit presentation
/// is requested (via either VS15/V16), that is always honored.
/// The reason we do this is because we assume that if a user
/// explicitly chosen a font face (hence it is "explicit" and
/// not "fallback"), they want to use any glyphs possible within that
/// font face. Fallback fonts on the other hand are picked as a
/// last resort, so we should prefer exactness if possible.
pub struct Entry {
    pub face: AnyFace,

    /// Whether this face is a fallback face, see
    /// main doc comment on Entry for more info.
    pub fallback: bool,

    /// Factor to multiply the collection size by for this face, or
    /// else the size adjustment that should be used to calculate
    /// once the face is loaded.
    ///
    /// This is computed when the face is loaded, based on a scale
    /// factor computed for an adjustment from the primary face to
    /// this one, which allows fallback fonts to be harmonized with
    /// the primary font by matching one of the metrics between them.
    // scale_factor: union(enum) { adjustment: SizeAdjustment, scale: f64 } = .{ .scale = 1.0 },
    pub scale_factor: ScaleFactor,
}

/// The `AnyFace` union nested in `Entry` (Rust has no nested type declarations).
// const AnyFace = union(enum) { deferred: DeferredFace, loaded: Face };
pub enum AnyFace {
    /// Not yet loaded.
    Deferred(DeferredFace),
    /// Loaded.
    Loaded(Face),
}

/// The `scale_factor` union nested in `Entry`.
#[derive(Clone, Copy, Debug)]
pub enum ScaleFactor {
    Adjustment(SizeAdjustment),
    Scale(f64),
}

impl Entry {
    // pub fn deinit(self: *Entry) void {
    pub fn deinit(&mut self) {
        // switch (self.face) { inline .deferred, .loaded => |*v| v.deinit() }
        match &mut self.face {
            AnyFace::Deferred(v) => v.deinit(),
            AnyFace::Loaded(v) => v.deinit(),
        }
    }

    /// If this face is loaded, then this returns the `Face`,
    /// otherwise returns null.
    // pub fn getLoaded(self: *Entry) ?*Face {
    pub fn get_loaded(&mut self) -> Option<*mut Face> {
        match &mut self.face {
            AnyFace::Deferred(_) => None,
            AnyFace::Loaded(face) => Some(face as *mut Face),
        }
    }

    /// True if the entry is deferred.
    // fn isDeferred(self: Entry) bool {
    #[allow(dead_code)]
    fn is_deferred(&self) -> bool {
        match &self.face {
            AnyFace::Deferred(_) => true,
            AnyFace::Loaded(_) => false,
        }
    }

    /// True if this face satisfies the given codepoint and presentation.
    // pub fn hasCodepoint(self: Entry, cp: u32, p_mode: PresentationMode) bool {
    pub fn has_codepoint(&self, cp: u32, p_mode: PresentationMode) -> bool {
        // return mode: switch (p_mode) { ... continue :mode ... };
        let mut mode = p_mode;
        loop {
            match mode {
                PresentationMode::Default(p) => {
                    if self.fallback {
                        // Fallback fonts require explicit presentation matching.
                        mode = PresentationMode::Explicit(p);
                        continue;
                    } else {
                        // Non-fallback fonts do not.
                        mode = PresentationMode::Any;
                        continue;
                    }
                }

                PresentationMode::Explicit(p) => {
                    return match &self.face {
                        AnyFace::Deferred(v) => v.has_codepoint(cp, Some(p)),

                        AnyFace::Loaded(face) => {
                            let index = match face.glyph_index(cp) {
                                Some(i) => i,
                                None => return false,
                            };
                            match p {
                                Presentation::Text => !face.is_color_glyph(index),
                                Presentation::Emoji => face.is_color_glyph(index),
                            }
                        }
                    };
                }

                PresentationMode::Any => {
                    return match &self.face {
                        AnyFace::Deferred(v) => v.has_codepoint(cp, None),

                        AnyFace::Loaded(face) => face.glyph_index(cp).is_some(),
                    };
                }
            }
        }
    }
}

/// A collection entry, which is either an owned `Entry` or an alias to one.
// pub const EntryOrAlias = union(enum) { entry: Entry, alias: *Entry };
pub enum EntryOrAlias {
    Entry(Entry),

    /// An alias to another entry. This is used to share the same face,
    /// avoid memory duplication. An alias must point to a non-alias entry.
    Alias(*mut Entry),
}

impl EntryOrAlias {
    /// Get a pointer to the underlying entry.
    // pub fn getEntry(self: *EntryOrAlias) *Entry {
    pub fn get_entry(&mut self) -> *mut Entry {
        match self {
            EntryOrAlias::Entry(v) => v as *mut Entry,
            EntryOrAlias::Alias(v) => *v,
        }
    }

    /// Get a const pointer to the underlying entry.
    // pub fn getConstEntry(self: *const EntryOrAlias) *const Entry {
    pub fn get_const_entry(&self) -> *const Entry {
        match self {
            EntryOrAlias::Entry(v) => v as *const Entry,
            EntryOrAlias::Alias(v) => *v as *const Entry,
        }
    }
}

/// The requested presentation for a codepoint.
// pub const PresentationMode = union(enum) { explicit: Presentation, default: Presentation, any: void };
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PresentationMode {
    /// The codepoint has an explicit presentation that is required,
    /// i.e. VS15/V16.
    Explicit(Presentation),

    /// The codepoint has no explicit presentation and we should use
    /// the presentation from the UCD.
    Default(Presentation),

    /// The codepoint can be any presentation.
    Any,
}

/// Options for adjusting the size of a face relative to the primary face.
// pub const SizeAdjustment = enum { none, ic_width, ex_height, cap_height, line_height };
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SizeAdjustment {
    /// Don't adjust the size for this face, use the original point size.
    None,
    /// Match ideograph character width with the primary face.
    IcWidth,
    /// Match ex height with the primary face.
    ExHeight,
    /// Match cap height with the primary face.
    CapHeight,
    /// Match line height with the primary face.
    LineHeight,
}

/// This represents a specific font in the collection.
///
/// The backing size of this packed struct represents the total number
/// of possible usable fonts in a collection. And the number of bits
/// used for the index and not the style represents the total number
/// of possible usable fonts for a given style.
///
/// The goal is to keep the size of this struct as small as practical. We
/// accept the limitations that this imposes so long as they're reasonable.
/// At the time of writing this comment, this is a 16-bit struct with 13
/// bits used for the index, supporting up to 8192 fonts per style. This
/// seems more than reasonable. There are synthetic scenarios where this
/// could be a limitation but I can't think of any that are practical.
///
/// If you somehow need more fonts per style, you can increase the size of
/// the Backing type and everything should just work fine.
// pub const Index = packed struct(Index.Backing) { ... };
//
// PORT NOTE: `packed struct(u16) { style: Style (u3), idx: u13 }` is modeled as a
// newtype over the `u16` backing so `@sizeOf(Index) == @sizeOf(u16)` (asserted by
// the `Index` test). In a Zig packed struct the FIRST field is the least
// significant bits, so `style` occupies bits 0..3 and `idx` bits 3..16;
// `int()`/`new` encode that layout, and `style()`/`idx()` decode it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Index(pub u16);

/// Sibling declarations of `Index` (Rust has no nested types). `IndexInt` is the
/// reference's `u13` index integer; here it is carried in a `u16` field.
pub mod index {
    /// The special-case fonts that we support.
    // pub const Special = enum(IndexInt) {
    //     // We start all special fonts at this index so they can be detected.
    //     pub const start = std.math.maxInt(IndexInt);
    //     /// Sprite drawing, this is rendered JIT using 2D graphics APIs.
    //     sprite = start,
    // };
    //
    // PORT NOTE: `IndexInt` is `u13`; its `maxInt` is `8191`. `Special.sprite` is
    // that value, so `special()` treats any `idx >= 8191` as special.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    #[repr(u16)]
    pub enum Special {
        /// Sprite drawing, this is rendered JIT using 2D graphics APIs.
        Sprite = super::Index::SPECIAL_START,
    }
}

impl Index {
    // const Backing = u16;
    // const backing_bits = @typeInfo(Backing).int.bits;  // 16
    //
    /// The number of bits we use for the index.
    // const idx_bits = backing_bits - @typeInfo(@typeInfo(Style).@"enum".tag_type).int.bits;
    // pub const IndexInt = @Type(.{ .int = .{ .signedness = .unsigned, .bits = idx_bits } });
    // (16 - 3 = 13.)
    pub const IDX_BITS: u32 = 13;

    /// `std.math.maxInt(IndexInt)` where `IndexInt = u13` → `8191`. Also the value
    /// of `Special.start`/`Special.sprite`.
    pub const SPECIAL_START: u16 = (1u16 << Self::IDX_BITS) - 1;

    /// Construct an index from a style and per-style index.
    // (models `.{ .style = ..., .idx = ... }`.)
    pub const fn new(style: Style, idx: u16) -> Index {
        Index((style as u16) | (idx << 3))
    }

    /// The style component (bits 0..3).
    pub fn style(self) -> Style {
        match self.0 & 0b111 {
            0 => Style::Regular,
            1 => Style::Bold,
            2 => Style::Italic,
            3 => Style::BoldItalic,
            // The tag is a u3 but only 0..=3 are valid `Style` values; the
            // reference relies on only valid styles ever being stored.
            _ => unreachable!(),
        }
    }

    /// The per-style index component (bits 3..16).
    pub fn idx(self) -> u16 {
        self.0 >> 3
    }

    /// Initialize a special font index.
    // pub fn initSpecial(v: Special) Index { return .{ .style = .regular, .idx = @intFromEnum(v) }; }
    pub const fn init_special(v: index::Special) -> Index {
        Index::new(Style::Regular, v as u16)
    }

    /// Convert to int
    // pub fn int(self: Index) Backing { return @bitCast(self); }
    pub fn int(self) -> u16 {
        self.0
    }

    /// Returns true if this is a "special" index which doesn't map to
    /// a real font face. We can still render it but there is no face for
    /// this font.
    // pub fn special(self: Index) ?Special {
    pub fn special(self) -> Option<index::Special> {
        // if (self.idx < Special.start) return null;
        if self.idx() < Self::SPECIAL_START {
            return None;
        }
        // return @enumFromInt(self.idx);
        Some(index::Special::Sprite)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::face::Options;
    use crate::{options, Backend};
    use rustty_datastruct::comparison::expect_approx_equal;
    use rustty_font::embedded;

    // PORT NOTE: the reference tests use Zig `try`; here fallible setup calls use
    // `.unwrap()` (a failed setup panics → the test fails, matching `try` in a Zig
    // test), and `try testing.expectXxx` become `assert!`/`assert_eq!`.

    // Helper: build face `Options` from a `DesiredSize` (the reference tests
    // construct faces with `.{ .size = ... }`).
    fn opts(size: DesiredSize) -> Options {
        Options {
            size,
            freetype_load_flags: crate::face::FREETYPE_LOAD_FLAGS_DEFAULT,
        }
    }

    // test { ... } — the `Index` packed-struct size/layout assertions.
    #[test]
    fn index_layout() {
        // We never want to take up more than a byte since font indexes are
        // everywhere so if we increase the size of this we'll dramatically
        // increase our memory usage.
        assert_eq!(core::mem::size_of::<u16>(), core::mem::size_of::<Index>());

        // Just so we're aware when this changes. The current maximum number
        // of fonts for a style is 13 bits or 8192 fonts.
        assert_eq!(13, Index::IDX_BITS);
    }

    // test init
    #[test]
    fn init_test() {
        let mut c = Collection::init();
        c.deinit();
    }

    // test "add full"
    #[test]
    fn add_full() {
        // This test is way too slow to run under Valgrind, unfortunately.
        // if (std.valgrind.runningOnValgrind() > 0) return error.SkipZigTest;
        //
        // PORT NOTE: rustty gates memory-safety on ASan (not Valgrind — R6); there
        // is no Valgrind runtime to detect here, so the skip guard is a no-op.

        let test_font = embedded::REGULAR;

        let mut lib = Library::init().unwrap();

        let mut c = Collection::init();

        for _ in 0..(Index::SPECIAL_START - 1) {
            c.add(
                Face::init(
                    lib,
                    test_font,
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

        let face = Face::init(
            lib,
            test_font,
            opts(DesiredSize {
                points: 12.0,
                xdpi: crate::face::DEFAULT_DPI,
                ydpi: crate::face::DEFAULT_DPI,
            }),
        )
        .unwrap();
        // We have to deinit it manually since the
        // collection doesn't do it if adding fails.
        // PORT NOTE: the reference `defer face.deinit()` frees the face the failed
        // `add` didn't take ownership of. Here `add` takes `face` by value and, on
        // the `CollectionFull` early return (before it stores anything), drops it —
        // releasing the underlying font. The reference's manual deinit is subsumed
        // by that move + `Drop`.
        assert!(matches!(
            c.add(
                face,
                AddOptions {
                    style: Style::Regular,
                    fallback: false,
                    size_adjustment: SizeAdjustment::None,
                },
            ),
            Err(Error::CollectionFull)
        ));

        c.deinit();
        lib.deinit();
    }

    // test "add deferred without loading options"
    #[test]
    fn add_deferred_without_loading_options() {
        let mut c = Collection::init();

        // This can be undefined because it should never be accessed.
        // PORT NOTE: the reference passes `undefined` for the deferred face; the
        // `load_options == null` check returns before the face is read. We build a
        // real (empty) `DeferredFace` since Rust has no `undefined`; it is never
        // accessed.
        assert!(matches!(
            c.add_deferred(
                DeferredFace::undefined(),
                AddOptions {
                    style: Style::Regular,
                    fallback: false,
                    size_adjustment: SizeAdjustment::None,
                },
            ),
            Err(Error::DeferredLoadingUnavailable)
        ));

        c.deinit();
    }

    // test getFace
    #[test]
    fn get_face() {
        let test_font = embedded::REGULAR;

        let mut lib = Library::init().unwrap();

        let mut c = Collection::init();

        let idx = c
            .add(
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

        {
            let face1 = c.get_face(idx).unwrap();
            let face2 = c.get_face(idx).unwrap();
            assert_eq!(face1 as usize, face2 as usize);
        }

        c.deinit();
        lib.deinit();
    }

    // test getIndex
    #[test]
    fn get_index() {
        let test_font = embedded::REGULAR;

        let mut lib = Library::init().unwrap();

        let mut c = Collection::init();

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

        // Should find all visible ASCII
        let mut i: u32 = 32;
        while i < 127 {
            let idx = c.get_index(i, Style::Regular, PresentationMode::Any);
            assert!(idx.is_some());
            i += 1;
        }

        // Should not find emoji
        {
            let idx = c.get_index(u32::from('🥸'), Style::Regular, PresentationMode::Any);
            assert!(idx.is_none());
        }

        c.deinit();
        lib.deinit();
    }

    // test completeStyles
    #[test]
    fn complete_styles() {
        let test_font = embedded::REGULAR;

        let mut lib = Library::init().unwrap();

        let mut c = Collection::init();
        c.load_options = Some(LoadOptions {
            library: lib,
            ..Default::default()
        });

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

        assert!(c
            .get_index(u32::from('A'), Style::Bold, PresentationMode::Any)
            .is_none());
        assert!(c
            .get_index(u32::from('A'), Style::Italic, PresentationMode::Any)
            .is_none());
        assert!(c
            .get_index(u32::from('A'), Style::BoldItalic, PresentationMode::Any)
            .is_none());
        c.complete_styles(FontSyntheticStyle::default()).unwrap();
        assert!(c
            .get_index(u32::from('A'), Style::Bold, PresentationMode::Any)
            .is_some());
        assert!(c
            .get_index(u32::from('A'), Style::Italic, PresentationMode::Any)
            .is_some());
        assert!(c
            .get_index(u32::from('A'), Style::BoldItalic, PresentationMode::Any)
            .is_some());

        c.deinit();
        lib.deinit();
    }

    // test setSize
    #[test]
    fn set_size() {
        let test_font = embedded::REGULAR;

        let mut lib = Library::init().unwrap();

        let mut c = Collection::init();
        c.load_options = Some(LoadOptions {
            library: lib,
            ..Default::default()
        });

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

        assert_eq!(12.0_f32, c.load_options.unwrap().size.points);
        c.set_size(DesiredSize {
            points: 24.0,
            xdpi: crate::face::DEFAULT_DPI,
            ydpi: crate::face::DEFAULT_DPI,
        })
        .unwrap();
        assert_eq!(24.0_f32, c.load_options.unwrap().size.points);

        c.deinit();
        lib.deinit();
    }

    // test hasCodepoint
    #[test]
    fn has_codepoint() {
        let test_font = embedded::REGULAR;

        let mut lib = Library::init().unwrap();

        let mut c = Collection::init();
        c.load_options = Some(LoadOptions {
            library: lib,
            ..Default::default()
        });

        let idx = c
            .add(
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

        assert!(c.has_codepoint(idx, u32::from('A'), PresentationMode::Any));
        assert!(!c.has_codepoint(idx, u32::from('🥸'), PresentationMode::Any));

        c.deinit();
        lib.deinit();
    }

    // test "hasCodepoint emoji default graphical"
    #[test]
    fn has_codepoint_emoji_default_graphical() {
        // if (options.backend != .fontconfig_freetype) return error.SkipZigTest;
        //
        // PORT NOTE: the reference early-returns `SkipZigTest` when the backend
        // isn't `fontconfig_freetype`; here the body runs only when it is (an
        // inverted guard, equivalent to the skip and clippy-clean). On the macOS
        // coretext gate the condition is false, so the test is a no-op skip.
        #[cfg(not(target_os = "macos"))]
        if options::BACKEND == Backend::FontconfigFreetype {
            let test_emoji = embedded::EMOJI;

            let mut lib = Library::init().unwrap();

            let mut c = Collection::init();
            c.load_options = Some(LoadOptions {
                library: lib,
                ..Default::default()
            });

            let idx = c
                .add(
                    Face::init(
                        lib,
                        test_emoji,
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

            assert!(!c.has_codepoint(idx, u32::from('A'), PresentationMode::Any));
            assert!(c.has_codepoint(idx, u32::from('🥸'), PresentationMode::Any));
            // TODO(fontmem): test explicit/implicit

            c.deinit();
            lib.deinit();
        }
    }

    // test "metrics"
    #[test]
    fn metrics() {
        let test_font = embedded::INCONSOLATA;

        let mut lib = Library::init().unwrap();

        let mut c = Collection::init();
        let size = DesiredSize {
            points: 12.0,
            xdpi: 96,
            ydpi: 96,
        };
        c.load_options = Some(LoadOptions {
            library: lib,
            size,
            ..Default::default()
        });

        c.add(
            Face::init(lib, test_font, opts(size)).unwrap(),
            AddOptions {
                style: Style::Regular,
                fallback: false,
                size_adjustment: SizeAdjustment::None,
            },
        )
        .unwrap();

        c.update_metrics().unwrap();

        // (see the reference for the derivation of these expected values.)
        expect_approx_equal(
            &Metrics {
                cell_width: 8,
                // The cell height is 17 px because the calculation is
                //  ascender - descender + gap
                // which, for inconsolata is
                //  859 - -190 + 0
                // font units, at 1000 units per em that works out to 1.049 em,
                // and 1em should be the point size * dpi scale, so 12 * (96/72)
                // which is 16, and 16 * 1.049 = 16.784, which finally is rounded
                // to 17.
                // The icon height is (2 * cap_height + face_height) / 3
                // = (2 * 623 + 1049) / 3 = 765, and 16 * 0.765 = 12.24.
                cell_height: 17,
                cell_baseline: 3,
                underline_position: 17,
                underline_thickness: 1,
                strikethrough_position: 10,
                strikethrough_thickness: 1,
                overline_position: 0,
                overline_thickness: 1,
                box_thickness: 1,
                // (reference omits `cursor_thickness`; its struct default is `1`.)
                cursor_thickness: 1,
                cursor_height: 17,
                icon_height: 16.784,
                icon_height_single: 12.24,
                face_width: 8.0,
                face_height: 16.784,
                face_y: -0.04,
            },
            &c.metrics.unwrap(),
        )
        .unwrap();

        // Resize should change metrics
        c.set_size(DesiredSize {
            points: 24.0,
            xdpi: 96,
            ydpi: 96,
        })
        .unwrap();
        expect_approx_equal(
            &Metrics {
                cell_width: 16,
                cell_height: 34,
                cell_baseline: 6,
                underline_position: 34,
                underline_thickness: 2,
                strikethrough_position: 19,
                strikethrough_thickness: 2,
                overline_position: 0,
                overline_thickness: 2,
                box_thickness: 2,
                cursor_thickness: 1,
                cursor_height: 34,
                icon_height: 33.568,
                icon_height_single: 24.48,
                face_width: 16.0,
                face_height: 33.568,
                face_y: -0.08,
            },
            &c.metrics.unwrap(),
        )
        .unwrap();

        c.deinit();
        lib.deinit();
    }

    // TODO: Also test CJK fallback sizing, we don't currently have a CJK test font.
    // test "adjusted sizes"
    #[test]
    fn adjusted_sizes() {
        let test_font = embedded::INCONSOLATA;
        let fallback = embedded::MONASPACE_NEON;
        let symbol = embedded::SYMBOLS_NERD_FONT;

        let mut lib = Library::init().unwrap();

        let mut c = Collection::init();
        let size = DesiredSize {
            points: 12.0,
            xdpi: 96,
            ydpi: 96,
        };
        c.load_options = Some(LoadOptions {
            library: lib,
            size,
            ..Default::default()
        });

        // Add our primary face.
        c.add(
            Face::init(lib, test_font, opts(size)).unwrap(),
            AddOptions {
                style: Style::Regular,
                fallback: false,
                size_adjustment: SizeAdjustment::None,
            },
        )
        .unwrap();

        c.update_metrics().unwrap();

        // inline for ([_][]const u8{ "ex_height", "cap_height" }) |metric|
        // PORT NOTE: Zig's `inline for` over the two metric names + `@field` is a
        // compile-time unroll; reproduced here by iterating a `SizeAdjustment` +
        // an accessor closure over `FaceMetrics` for each of the two metrics.
        #[allow(clippy::type_complexity)]
        let cases: [(SizeAdjustment, fn(FaceMetrics) -> Option<f64>); 2] = [
            (SizeAdjustment::ExHeight, |m: FaceMetrics| m.ex_height),
            (SizeAdjustment::CapHeight, |m: FaceMetrics| m.cap_height),
        ];
        for (adjustment, get_metric) in cases {
            // Add the fallback face with the chosen adjustment metric.
            let fallback_idx = c
                .add(
                    Face::init(lib, fallback, opts(size)).unwrap(),
                    AddOptions {
                        style: Style::Regular,
                        fallback: false,
                        size_adjustment: adjustment,
                    },
                )
                .unwrap();

            // The chosen metric should match.
            {
                let primary_metrics =
                    unsafe { &*c.get_face(Index::new(Style::Regular, 0)).unwrap() }.get_metrics();
                let fallback_metrics = unsafe { &*c.get_face(fallback_idx).unwrap() }.get_metrics();

                // We accept anything within half a pixel.
                assert!(
                    (get_metric(primary_metrics).unwrap() - get_metric(fallback_metrics).unwrap())
                        .abs()
                        <= 0.5
                );
            }

            // Resize should keep that relationship.
            c.set_size(DesiredSize {
                points: 37.0,
                xdpi: 96,
                ydpi: 96,
            })
            .unwrap();
            {
                let primary_metrics =
                    unsafe { &*c.get_face(Index::new(Style::Regular, 0)).unwrap() }.get_metrics();
                let fallback_metrics = unsafe { &*c.get_face(fallback_idx).unwrap() }.get_metrics();

                // We accept anything within half a pixel.
                assert!(
                    (get_metric(primary_metrics).unwrap() - get_metric(fallback_metrics).unwrap())
                        .abs()
                        <= 0.5
                );
            }

            // Reset size for the next iteration
            c.set_size(size).unwrap();
        }

        {
            // A reference metric of "none" should leave the size unchanged.
            let fallback_idx = c
                .add(
                    Face::init(lib, fallback, opts(size)).unwrap(),
                    AddOptions {
                        style: Style::Regular,
                        fallback: false,
                        size_adjustment: SizeAdjustment::None,
                    },
                )
                .unwrap();

            assert_eq!(
                unsafe { &*c.get_face(Index::new(Style::Regular, 0)).unwrap() }
                    .size
                    .points,
                unsafe { &*c.get_face(fallback_idx).unwrap() }.size.points,
            );

            // Resize should keep that.
            c.set_size(DesiredSize {
                points: 37.0,
                xdpi: 96,
                ydpi: 96,
            })
            .unwrap();

            assert_eq!(
                unsafe { &*c.get_face(Index::new(Style::Regular, 0)).unwrap() }
                    .size
                    .points,
                unsafe { &*c.get_face(fallback_idx).unwrap() }.size.points,
            );

            // Reset collection size
            c.set_size(size).unwrap();
        }

        // Add the symbol face.
        let symbol_idx = c
            .add(
                Face::init(lib, symbol, opts(size)).unwrap(),
                AddOptions {
                    style: Style::Regular,
                    fallback: false,
                    size_adjustment: SizeAdjustment::ExHeight,
                },
            )
            .unwrap();

        // Test fallback to lineHeight() (ex_height and cap_height not defined in symbols font).
        {
            let primary_metrics =
                unsafe { &*c.get_face(Index::new(Style::Regular, 0)).unwrap() }.get_metrics();
            let symbol_metrics = unsafe { &*c.get_face(symbol_idx).unwrap() }.get_metrics();

            // We accept anything within half a pixel.
            assert!((primary_metrics.line_height() - symbol_metrics.line_height()).abs() <= 0.5);
        }

        c.deinit();
        lib.deinit();
    }

    // test "face metrics"
    #[test]
    fn face_metrics() {
        // The web canvas backend doesn't calculate face metrics, only cell metrics
        // if (options.backend != .web_canvas) return error.SkipZigTest;
        //
        // PORT NOTE: `web_canvas` is never the active backend for the macOS/Linux
        // parity targets, so this test always takes the skip path (its body needs
        // the `cozette`/`geist_mono` embeds + web_canvas measurements). The full
        // body lands with the wasm/web_canvas backend port. The `!= web_canvas`
        // skip is expressed as an inverted `== web_canvas` guard (clippy-clean).
        // The reference body (DEFERRED) is preserved verbatim:
        //
        //   const narrowFont = font.embedded.cozette;
        //   const wideFont = font.embedded.geist_mono;
        //   var lib = try Library.init(alloc); defer lib.deinit();
        //   var c = init(); defer c.deinit(alloc);
        //   const size: DesiredSize = .{ .points = 12, .xdpi = 96, .ydpi = 96 };
        //   c.load_options = .{ .library = lib, .size = size };
        //   const narrowIndex = try c.add(alloc, try .init(lib, narrowFont, .{ .size = size }), ...);
        //   const wideIndex = try c.add(alloc, try .init(lib, wideFont, .{ .size = size }), ...);
        //   const narrowMetrics: font.Metrics.FaceMetrics = (try c.getFace(narrowIndex)).getMetrics();
        //   const wideMetrics: font.Metrics.FaceMetrics = (try c.getFace(wideIndex)).getMetrics();
        //   // Verify provided/measured metrics. Measured
        //   // values are backend-dependent due to hinting.
        //   const narrowMetricsExpected = font.Metrics.FaceMetrics{ ... };
        //   const wideMetricsExpected = font.Metrics.FaceMetrics{ ... };
        //   inline for (.{ narrowMetricsExpected, wideMetricsExpected }, .{ narrowMetrics, wideMetrics })
        //       |metricsExpected, metricsActual| { try comparison.expectApproxEqual(metricsExpected, metricsActual); }
        //   // Verify estimated metrics. icWidth() should equal the smaller of
        //   // 2 * cell_width and ascii_height. For a narrow (wide) font, the
        //   // smaller quantity is the former (latter).
        //   try std.testing.expectEqual(2 * narrowMetrics.cell_width, narrowMetrics.icWidth());
        //   try std.testing.expectEqual(wideMetrics.ascii_height, wideMetrics.icWidth());
        if options::BACKEND == Backend::WebCanvas {
            // (body DEFERRED with the web_canvas backend)
        }
    }
}
