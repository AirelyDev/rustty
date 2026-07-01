//! `discovery` seam.
//!
//! PORT NOTE (glue): the reference `src/font/discovery.zig` (1349 loc) is a
//! separate file — NOT one of the three files ported in this batch — and is
//! `switch(options.backend)` over a Fontconfig, a CoreText, and a Windows
//! discoverer plus the `Descriptor`/`Discover` types. `Collection`,
//! `DeferredFace`, and `CodepointResolver` reference three things from it:
//!
//!   * `Descriptor` — the font search descriptor (stored in `CodepointResolver`'s
//!     descriptor cache, built for fallback searches),
//!   * `Discover` — the active backend discoverer type,
//!   * `CoreText.init/.discover` — used by the `DeferredFace` "coretext" test.
//!
//! Like the `ft`/`hb`/`stb`/`library`/`uucode` glue modules, this is a thin
//! stand-in for exactly those surfaces (the macOS CoreText path), superseded by
//! the full `discovery.zig` port later. Fontconfig discovery (the Linux default)
//! is DEFERRED (documented): on non-macOS targets `Discover`/`Descriptor` resolve
//! to deferred stubs so the crate compiles, but the Fontconfig discoverer itself
//! lands with the full port. This module is not part of any reference file's test
//! contract.

use crate::face::Variation;

/// The font descriptor used to search for a font.
// PORT: `discovery.zig`'s `Descriptor` (the subset the ported files touch).
#[derive(Clone, Debug, Default)]
pub struct Descriptor {
    /// Font family to search for. This can be a fully qualified font
    /// name such as "Fira Code", "monospace", "serif", etc.
    // family: ?[:0]const u8 = null,
    pub family: Option<Vec<u8>>,

    /// Specific font style to search for. This will filter the style
    /// string the font advertises.
    // style: ?[:0]const u8 = null,
    pub style: Option<Vec<u8>>,

    /// A codepoint that this font must be able to render.
    // codepoint: u32 = 0,
    pub codepoint: u32,

    /// Font size in points that the font should support.
    // size: f32 = 0,
    pub size: f32,

    /// True if we want to search specifically for a font that supports
    /// bold, italic, or both.
    // bold: bool = false,
    pub bold: bool,
    // italic: bool = false,
    pub italic: bool,

    /// Whether the font should be monospaced.
    // monospace: bool = false,
    pub monospace: bool,

    /// Variation axes to apply to the font. This also impacts searching
    /// because the variedvariations can impact features.
    // variations: []const font.face.Variation = &.{},
    pub variations: Vec<Variation>,
}

impl Descriptor {
    /// Deep copy of the struct. The given allocator is expected to
    /// be an arena allocator of some sort since the descriptor
    /// itself doesn't support fine-grained deallocation of fields.
    // pub fn clone(self: *const Descriptor, alloc: Allocator) !Descriptor {
    //
    // PORT NOTE: rustty uses owned `Vec`/`String` fields, so `clone` is the derived
    // deep clone; `alloc` is not threaded (fields free themselves).
    pub fn clone_desc(&self) -> Descriptor {
        self.clone()
    }

    /// Hash the descriptor into the given hasher.
    // pub fn hash(self: Descriptor, hasher: anytype) void { ... }
    //
    // (Named `hash` to match the reference; not the std `Hash::hash` trait method.)
    #[allow(clippy::should_implement_trait)]
    pub fn hash<H: core::hash::Hasher>(&self, hasher: &mut H) {
        use core::hash::Hash;
        // autoHashStrat(hasher, self.family, .Deep);
        self.family.hash(hasher);
        // autoHashStrat(hasher, self.style, .Deep);
        self.style.hash(hasher);
        // autoHash(hasher, self.codepoint);
        self.codepoint.hash(hasher);
        // autoHash(hasher, @as(u32, @bitCast(self.size)));
        self.size.to_bits().hash(hasher);
        // autoHash(hasher, self.bold);
        self.bold.hash(hasher);
        // autoHash(hasher, self.italic);
        self.italic.hash(hasher);
        // autoHash(hasher, self.monospace);
        self.monospace.hash(hasher);
    }

    /// Returns a hash code that can be used to uniquely identify this
    /// action.
    // pub fn hashcode(self: Descriptor) u64 {
    pub fn hashcode(&self) -> u64 {
        // var hasher = std.hash.Wyhash.init(0);
        //
        // PORT NOTE: the reference seeds `std.hash.Wyhash`; rustty has no Wyhash
        // port in this tier, so the stdlib `DefaultHasher` is used. The descriptor
        // cache keyed by this hashcode is not exercised by any ported test (the
        // resolver tests never set a codepoint map / discoverer), so the exact hash
        // algorithm is not observable; a full Wyhash port lands with `discovery.zig`.
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.hash(&mut hasher);
        core::hash::Hasher::finish(&hasher)
    }
}

/// Bridge a `CodepointMap` descriptor (the thin seam in `rustty-font`, carrying
/// only `family`/`monospace`) into a full discovery `Descriptor`. The reference
/// `CodepointMap.get` returns a `discovery.Descriptor` directly (they are the same
/// type); rustty seams the below-vt `CodepointMap` with a two-field descriptor, so
/// this bridge fills the remaining defaults.
impl From<rustty_font::codepoint_map::Descriptor> for Descriptor {
    fn from(d: rustty_font::codepoint_map::Descriptor) -> Self {
        Descriptor {
            family: d.family,
            monospace: d.monospace,
            ..Descriptor::default()
        }
    }
}

// ---------------------------------------------------------------------------
// The active discoverer type.
// pub const Discover = switch (options.backend) { .coretext* => CoreText, ... };
// ---------------------------------------------------------------------------

/// The active backend discoverer. macOS default `coretext` → `CoreText`.
#[cfg(target_os = "macos")]
pub type Discover = CoreText;

/// Deferred discoverer stub for non-macOS targets (the Fontconfig discoverer is
/// DEFERRED). Present only so `CodepointResolver`/`Collection` type-check; the
/// real Fontconfig discoverer lands with the full `discovery.zig` port.
#[cfg(not(target_os = "macos"))]
pub type Discover = Fontconfig;

/// DEFERRED Fontconfig discoverer stub (Linux default). See module note.
#[cfg(not(target_os = "macos"))]
pub struct Fontconfig;

#[cfg(target_os = "macos")]
pub use coretext_impl::{CoreText, DiscoverIterator, Error};

#[cfg(target_os = "macos")]
mod coretext_impl {
    use super::Descriptor;
    use crate::deferred_face::{CoreTextData, DeferredFace};
    use crate::library::Library;
    use core::ptr::NonNull;
    use objc2_core_foundation::{CFArray, CFRetained};
    use objc2_core_text::{CTFont, CTFontCollection, CTFontDescriptor};

    /// The CoreText discoverer. Holds no state.
    // pub const CoreText = struct { ... };
    pub struct CoreText;

    /// The error set for CoreText discovery.
    #[derive(Debug)]
    pub enum Error {
        CoreTextFailure,
    }

    impl CoreText {
        // pub fn init(lib: Library) CoreText { _ = lib; return .{}; }
        pub fn init(_lib: Library) -> CoreText {
            // Required for the "interface" but does nothing for CoreText.
            CoreText
        }

        // pub fn deinit(self: *CoreText) void { _ = self; }
        pub fn deinit(&mut self) {}

        /// Discover fonts from a descriptor. This returns an iterator that can
        /// be used to build up the deferred fonts.
        // pub fn discover(self, alloc, desc: Descriptor) !DiscoverIterator {
        //
        // PORT NOTE (glue): the reference builds a CoreText descriptor from the
        // full attribute set, wraps it in a collection, fetches the matching
        // descriptors, then runs `copyMatchingDescriptors` + a `Score`-based
        // `sortMatchingDescriptors` to order them by quality. This stand-in builds
        // the family descriptor and returns the collection's matches in CoreText's
        // order (the elaborate scoring/sort is part of the full `discovery.zig`
        // port). Sufficient for the `DeferredFace` "coretext" test, which only
        // needs the first matching face for a family.
        pub fn discover(&self, desc: Descriptor) -> Result<DiscoverIterator, Error> {
            // Build our pattern that we'll search for: a descriptor keyed by the
            // requested family name.
            let family = desc.family.clone().unwrap_or_default();
            let family_str = String::from_utf8_lossy(&family);
            let cf_name =
                crate::deferred_face::new_cfstring(&family_str).ok_or(Error::CoreTextFailure)?;
            let ct_desc =
                unsafe { CTFontDescriptor::with_name_and_size(&cf_name, desc.size as f64) };

            // Our descriptors have to be in an array.
            let descs: [*const CTFontDescriptor; 1] = [&*ct_desc];
            let desc_arr = unsafe {
                CFArray::new(
                    None,
                    descs.as_ptr() as *mut *const core::ffi::c_void,
                    1,
                    core::ptr::null(),
                )
            }
            .ok_or(Error::CoreTextFailure)?;

            // Build our collection.
            let set = unsafe { CTFontCollection::with_font_descriptors(Some(&desc_arr), None) };
            let list: Option<CFRetained<CFArray>> = unsafe { set.matching_font_descriptors() };

            let mut zig_list: Vec<CFRetained<CTFontDescriptor>> = Vec::new();
            if let Some(list) = list {
                let count = CFArray::count(&list);
                for i in 0..count {
                    let ptr = unsafe { CFArray::value_at_index(&list, i) };
                    if ptr.is_null() {
                        continue;
                    }
                    // Retain: the array owns the value; we take our own reference.
                    let d: CFRetained<CTFontDescriptor> = unsafe {
                        CFRetained::retain(NonNull::new_unchecked(ptr as *mut CTFontDescriptor))
                    };
                    zig_list.push(d);
                }
            }

            Ok(DiscoverIterator {
                list: zig_list,
                variations: desc.variations,
                i: 0,
            })
        }

        /// Discover a fallback font for a codepoint. (Glue: delegates to the
        /// family search; the CJK/codepoint-specific `CTFontCreateForString`
        /// fallbacks land with the full port.)
        // pub fn discoverFallback(self, alloc, collection, desc) !DiscoverIterator {
        pub fn discover_fallback(
            &self,
            _collection: *mut crate::collection::Collection,
            desc: Descriptor,
        ) -> Result<DiscoverIterator, Error> {
            self.discover(desc)
        }
    }

    /// Iterator over the matching descriptors, yielding `DeferredFace`s.
    // pub const DiscoverIterator = struct { ... };
    pub struct DiscoverIterator {
        list: Vec<CFRetained<CTFontDescriptor>>,
        variations: Vec<crate::face::Variation>,
        i: usize,
    }

    impl DiscoverIterator {
        // pub fn deinit(self: *DiscoverIterator) void { ... }
        pub fn deinit(&mut self) {
            self.list.clear();
        }

        // pub fn next(self: *DiscoverIterator) !?DeferredFace {
        // (Named `next` to match the reference iterator; not `Iterator::next`,
        // whose signature — no `Result`, no external `deinit` — doesn't fit.)
        #[allow(clippy::should_implement_trait)]
        pub fn next(&mut self) -> Result<Option<DeferredFace>, Error> {
            if self.i >= self.list.len() {
                return Ok(None);
            }

            // Create our font. We need a size to initialize it so we use size
            // 12 but we will alter the size later.
            let desc = &self.list[self.i];
            let font: CFRetained<CTFont> =
                unsafe { CTFont::with_font_descriptor(desc, 12.0, core::ptr::null()) };

            // Increment after we return.
            self.i += 1;

            Ok(Some(DeferredFace {
                ct: Some(CoreTextData {
                    font,
                    variations: self.variations.clone(),
                }),
            }))
        }
    }
}
