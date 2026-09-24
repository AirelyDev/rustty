//! Font discovery.
// PORTS: ghostty `src/font/discovery.zig` (1349 loc, 8 tests) — font discovery.
// The active backend on macOS is the CoreText discoverer (`CTFontDescriptor` /
// `CTFontCollection`), fully ported here. The Fontconfig discoverer (the Linux
// default) is typed here with its reference body recorded verbatim; the Windows
// FreeType-scan discoverer (`freetype_windows` backend) is LIVE as of WS20.
// `Descriptor` (the search descriptor) and both conversions —
// `toCoreTextDescriptor` and `toFcPattern` (Fontconfig, gated to the fontconfig
// backend class) — are ported.
//
// PORT NOTE (imports): the reference imports are
//   const std = @import("std");
//   const builtin = @import("builtin");
//   const Allocator = std.mem.Allocator;
//   const assert = @import("../quirks.zig").inlineAssert;
//   const fontconfig = @import("fontconfig");
//   const macos = @import("macos");
//   const opentype = @import("opentype.zig");
//   const options = @import("main.zig").options;
//   const Collection = @import("main.zig").Collection;
//   const DeferredFace = @import("main.zig").DeferredFace;
//   const Face = @import("main.zig").Face;
//   const Library = @import("main.zig").Library;
//   const Presentation = @import("main.zig").Presentation;
//   const Variation = @import("main.zig").face.Variation;
//   const log = std.log.scoped(.discovery);
//     -> `LOG_SCOPE` below, carried through the `log` facade's `target:`
//        (`.port/parity-closure/DECISION-logging.md` §6).
// Mapped per memory `apc-glyf-font-layering-d1` + the F3 kickoff:
//   * `macos.foundation.*` -> `objc2_core_foundation`
//     (CFString/CFNumber/CFDictionary/CFArray/CFCharacterSet/CFRange),
//   * `macos.text.*`       -> `objc2_core_text`
//     (CTFontDescriptor/CTFontCollection/CTFont + the attribute-key statics),
//   * `opentype.*`         -> the PURE `rustty-font` leaf (`Head`/`OS2` parsers),
//   * `fontconfig`         -> Linux discovery, DEFERRED (recorded as comments),
//   * `assert`             -> `rustty_util::quirks::inlineAssert`,
//   * `log`                -> no logging facade wired into this crate; every
//     `log.*` call is ported as a verbatim comment and is a no-op.

/// The reference's `std.log.scoped(.discovery)` tag, carried through the `log`
/// facade's `target:` (`.port/parity-closure/DECISION-logging.md` §6).
///
/// NOTE (task-14c-4 fix round 1, F4): no LEDGER row named this file. Its two
/// live reference `log.warn` sites were found by enumerating the reference's own
/// calls per file (PROCEDURE.md blind spot 3), not from a row's cited anchors.
/// Recorded as finding **N6** in `.port/parity-closure/NEW-FINDINGS.md`.
#[cfg(target_os = "macos")]
const LOG_SCOPE: &str = "discovery";

use crate::face::Variation;

/// Discover implementation for the compile options.
// pub const Discover = switch (options.backend) {
//     .freetype => void, // no discovery
//     .freetype_windows => Windows,
//     .fontconfig_freetype => Fontconfig,
//     .web_canvas => void, // no discovery
//     .coretext,
//     .coretext_freetype,
//     .coretext_harfbuzz,
//     .coretext_noshape,
//     => CoreText,
// };
//
// PORT NOTE: the `switch` above is preserved verbatim as a reference; it is
// modeled here with `cfg(target_os)` matching `backend.zig`'s `Backend.default`
// (macOS default `coretext` → `CoreText`; Linux default `fontconfig_freetype` →
// `Fontconfig`). Parity targets both defaults.
/// The active backend discoverer. macOS default `coretext` → `CoreText`.
#[cfg(target_os = "macos")]
pub type Discover = CoreText;

/// Windows default `freetype_windows` → `Windows` (the FreeType directory scan).
/// Mirrors `Backend::default`'s `.windows => .freetype_windows` arm, which exists
/// precisely so Windows never pulls fontconfig (and its libxml2 dependency).
#[cfg(all(target_os = "windows", not(target_arch = "wasm32")))]
pub type Discover = Windows;

/// Linux/BSD default `fontconfig_freetype` → `Fontconfig` (the live Fontconfig
/// discoverer). On wasm32 this resolves to the `wasm_noop` `Fontconfig` instead
/// (the `.web_canvas => void` arm, which genuinely means "no discovery").
#[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
pub type Discover = Fontconfig;

// ---------------------------------------------------------------------------
// Fontconfig (Linux default `fontconfig_freetype`) discoverer.
//
// PORT NOTE: this is the LIVE Fontconfig discoverer (`discovery.zig`'s
// `Fontconfig`) — `init`/`deinit`/`discover`/`discoverFallback` and the
// `DiscoverIterator{next,deinit}` are all fully ported below; no
// `unimplemented!()` leaf remains on this path. `Collection`,
// `CodepointResolver` and `SharedGridSet` name those methods unconditionally
// (the reference comptime-elides them when `Discover == void`, but Rust
// type-checks them on every target), and they bind against these real bodies.
//
// The wasm arm further below is the one that stays a FUNCTIONAL noop, because
// `.web_canvas => void` genuinely means "no discovery".
#[cfg(all(
    not(target_os = "macos"),
    not(target_os = "windows"),
    not(target_arch = "wasm32")
))]
pub use fontconfig_impl::{DiscoverIterator, Error, Fontconfig};

#[cfg(all(
    not(target_os = "macos"),
    not(target_os = "windows"),
    not(target_arch = "wasm32")
))]
mod fontconfig_impl {
    use super::Descriptor;
    use crate::collection::Collection;
    use crate::deferred_face::{DeferredFace, FontconfigData};
    use crate::face::{FaceError, Variation};
    use crate::fc;
    use crate::library::Library;
    use rustty_util::quirks::inlineAssert as assert;

    /// The error set for Fontconfig discovery.
    // PORT NOTE: the reference errors are the inferred union of the AD HOC
    // `error.FontConfigFailed` raised at `discovery.zig:278` (the `fontSort`
    // non-`.match`), the package-level `fontconfig.Error` propagated from
    // `fontRenderPrepare`/`pattern.get` in `DiscoverIterator.next`
    // (`discovery.zig:320-333`), `Allocator.Error`, and the backend `Face`/
    // `Collection` errors threaded through `discoverFallback`. Modeled as one enum
    // with `From` conversions so `?` composes them — the same shape as the macOS
    // `coretext_impl::Error`, so `SharedGridSet`'s `Discovery(discovery::Error)`
    // variant composes identically on both targets.
    //
    // TWO-ERROR-ATOMS (R1): `FontConfigFailed` (capital "C") is the AD HOC
    // discovery-layer atom from `discovery.zig:278`; `Fc(fc::Error)` wraps the
    // DISTINCT package-level `fontconfig.Error` (whose failure atom is
    // `FontconfigFailed`, lowercase "c" — see `fc.rs`'s ERROR-ATOM NOTE). Both are
    // kept; collapsing them would lose fidelity.
    #[derive(Debug)]
    pub enum Error {
        /// `error.FontConfigFailed` — the `fontSort` call did not return `.match`
        /// (`discovery.zig:278`).
        FontConfigFailed,
        /// The package-level `fontconfig.Error` propagated from
        /// `fontRenderPrepare`/`pattern.get`.
        Fc(fc::Error),
        /// `Allocator.Error`.
        OutOfMemory,
        Face(FaceError),
        Collection(crate::collection::Error),
    }

    impl From<fc::Error> for Error {
        fn from(e: fc::Error) -> Self {
            Error::Fc(e)
        }
    }

    impl From<FaceError> for Error {
        fn from(e: FaceError) -> Self {
            Error::Face(e)
        }
    }

    impl From<crate::collection::Error> for Error {
        fn from(e: crate::collection::Error) -> Self {
            Error::Collection(e)
        }
    }

    /// The Fontconfig discoverer (Linux default backend).
    // pub const Fontconfig = struct { fc_config: *fontconfig.Config, ... };
    pub struct Fontconfig {
        /// The loaded Fontconfig configuration (config + scanned system fonts).
        /// Manually managed (`destroy`d in `deinit`), matching the reference's
        /// `fc_config: *fontconfig.Config`.
        // fc_config: *fontconfig.Config,
        fc_config: *mut fc::Config,
    }

    impl Fontconfig {
        // pub fn init(lib: Library) Fontconfig {
        //     _ = lib;
        //     // safe to call multiple times and concurrently
        //     _ = fontconfig.init();
        //     return .{ .fc_config = fontconfig.initLoadConfigAndFonts() };
        // }
        pub fn init(_lib: Library) -> Fontconfig {
            // safe to call multiple times and concurrently
            let _ = fc::init();
            Fontconfig {
                fc_config: fc::init_load_config_and_fonts(),
            }
        }

        // pub fn deinit(self: *Fontconfig) void { self.fc_config.destroy(); }
        pub fn deinit(&mut self) {
            // SAFETY: `fc_config` is the non-null config from
            // `init_load_config_and_fonts`, not yet destroyed.
            unsafe { (*self.fc_config).destroy() };
        }

        /// Discover fonts from a descriptor. This returns an iterator that can
        /// be used to build up the deferred fonts.
        // pub fn discover(self: *const Fontconfig, alloc: Allocator, desc: Descriptor) !DiscoverIterator
        //
        // PORT NOTE: the reference threads `alloc`; rustty uses the global
        // allocator, so `alloc` is omitted (matching `CoreText::discover`).
        pub fn discover(&self, desc: Descriptor) -> Result<DiscoverIterator, Error> {
            // Build our pattern that we'll search for
            // const pat = desc.toFcPattern();
            // errdefer pat.destroy();
            let pat = desc.to_fc_pattern();

            // SAFETY: `fc_config` is the non-null loaded config; `pat` is the
            // freshly created, non-null pattern from `to_fc_pattern` (owned here).
            let config = unsafe { &mut *self.fc_config };
            let pat_ref = unsafe { &mut *pat };

            // assert(self.fc_config.substituteWithPat(pat, .pattern));
            assert(config.substitute_with_pat(pat_ref, fc::MatchKind::Pattern));
            // pat.defaultSubstitute();
            pat_ref.default_substitute();

            // Search
            // const res = self.fc_config.fontSort(pat, false, null);
            let res = config.font_sort(pat_ref, false, None);
            // if (res.result != .match) return error.FontConfigFailed;
            if res.result != fc::Result::Match {
                // errdefer pat.destroy(); — the search failed, so free the pattern.
                pat_ref.destroy();
                return Err(Error::FontConfigFailed);
            }
            // errdefer res.fs.destroy();

            // .fonts = res.fs.fonts(),
            //
            // PORT NOTE: the reference borrows the fontset's `fonts()` slice; rustty
            // copies the (raw) pattern pointers into an owned `Vec` so the iterator
            // need not carry a self-referential borrow of `set`. The pointers remain
            // valid because `set` is kept alive (and destroyed) by the iterator.
            // SAFETY: `res.fs` is the non-null font set returned for a `.match`.
            let fonts = unsafe { (*res.fs).fonts() }.to_vec();

            Ok(DiscoverIterator {
                config: self.fc_config,
                pattern: pat,
                set: res.fs,
                fonts,
                variations: desc.variations,
                i: 0,
            })
        }

        // pub fn discoverFallback(self, alloc, collection, desc) !DiscoverIterator {
        //     _ = collection;
        //     return try self.discover(alloc, desc);
        // }
        pub fn discover_fallback(
            &self,
            _collection: *mut Collection,
            desc: Descriptor,
        ) -> Result<DiscoverIterator, Error> {
            self.discover(desc)
        }
    }

    /// The Fontconfig discovery iterator.
    // pub const DiscoverIterator = struct { config, pattern, set, fonts, variations, i };
    pub struct DiscoverIterator {
        // config: *fontconfig.Config,
        config: *mut fc::Config,
        // pattern: *fontconfig.Pattern,
        pattern: *mut fc::Pattern,
        // set: *fontconfig.FontSet,
        set: *mut fc::FontSet,
        // fonts: []*fontconfig.Pattern,
        fonts: Vec<*mut fc::Pattern>,
        // variations: []const Variation,
        variations: Vec<Variation>,
        // i: usize,
        i: usize,
    }

    #[cfg(test)]
    thread_local! {
        /// Test-only, PER-THREAD counter of how many `deinit` calls actually
        /// destroyed their native handles (idempotent no-op calls don't count).
        /// Thread-local so the `Drop` proving test below is unaffected by the
        /// other tests in this crate discovering fonts in parallel.
        static DEINIT_DESTROYS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    }

    /// Test-only accessor for this thread's `deinit`-destroy count.
    #[cfg(test)]
    pub(super) fn deinit_destroys() -> usize {
        DEINIT_DESTROYS.with(|c| c.get())
    }

    // PORT NOTE (W9F-01): the reference relies on `defer disco_it.deinit()` at
    // every construction site (`SharedGridSet.zig:197,228`,
    // `CodepointResolver.zig:182`) to free the owned `FcFontSet`/`FcPattern`.
    // Rust's equivalent "runs on every scope exit" primitive is `Drop`, so that
    // `defer` is expressed here ONCE instead of being re-materialized at each
    // exit path (the port's call sites missed the `?`-propagation, `break` and
    // early-`return` exits, leaking both handles). The explicit `deinit()` calls
    // the port already has stay valid — they become the destroy, and the drop a
    // no-op — via the idempotence below.
    impl Drop for DiscoverIterator {
        fn drop(&mut self) {
            self.deinit();
        }
    }

    impl DiscoverIterator {
        // pub fn deinit(self: *DiscoverIterator) void {
        //     self.set.destroy();
        //     self.pattern.destroy();
        //     self.* = undefined;
        // }
        pub fn deinit(&mut self) {
            // PORT NOTE (W9F-01): idempotent. The reference's `self.* = undefined`
            // makes a second `deinit` illegal (and its `defer` guarantees exactly
            // one call); rustty instead nulls the handles so that the materialized
            // explicit `deinit()` calls at the call sites AND the `Drop` above
            // (which makes the reference's `defer disco_it.deinit()` true on
            // EVERY exit — `?` propagation, `break`, early `return`) together
            // destroy the native handles exactly once.
            if self.set.is_null() {
                // Already deinited: nothing owned, nothing to destroy.
                return;
            }
            // SAFETY: `set`/`pattern` are the non-null handles this iterator owns
            // (from `discover`), not yet destroyed.
            unsafe {
                (*self.set).destroy();
                (*self.pattern).destroy();
            }
            self.set = std::ptr::null_mut();
            self.pattern = std::ptr::null_mut();
            // self.* = undefined; — the `Vec` fields drop; the raw handles above
            // are consumed by `destroy`. `fonts` borrowed its pattern pointers
            // from the (now destroyed) set, so it is cleared too: a post-deinit
            // `next()` then yields `None` instead of reading freed patterns.
            self.fonts.clear();
            self.i = 0;
            #[cfg(test)]
            DEINIT_DESTROYS.with(|c| c.set(c.get() + 1));
        }

        // pub fn next(self: *DiscoverIterator) fontconfig.Error!?DeferredFace {
        // (Named `next` to match the reference iterator; not `Iterator::next`,
        // whose signature — no `Result`, no external `deinit` — doesn't fit.)
        #[allow(clippy::should_implement_trait)]
        pub fn next(&mut self) -> Result<Option<DeferredFace>, Error> {
            // if (self.i >= self.fonts.len) return null;
            if self.i >= self.fonts.len() {
                return Ok(None);
            }

            // Get the copied pattern from our fontset that has the
            // attributes configured for rendering.
            // const font_pattern = try self.config.fontRenderPrepare(
            //     self.pattern,
            //     self.fonts[self.i],
            // );
            // errdefer font_pattern.destroy();
            //
            // SAFETY: `config`/`pattern` are the iterator's non-null handles;
            // `fonts[self.i]` is a non-null pattern owned by `set` (alive here).
            let config = unsafe { &mut *self.config };
            let pattern = unsafe { &*self.pattern };
            let font = unsafe { &*self.fonts[self.i] };
            let font_pattern = config.font_render_prepare(pattern, font)?;

            // return DeferredFace{ .fc = .{
            //     .pattern = font_pattern,
            //     .charset = (try font_pattern.get(.charset, 0)).char_set,
            //     .langset = (try font_pattern.get(.lang, 0)).lang_set,
            //     .variations = self.variations,
            // } };
            //
            // The two `get`s propagate `fontconfig.Error` exactly as the reference's
            // `try`; on any such error the render-prepared pattern must be destroyed
            // (the reference's `errdefer font_pattern.destroy()`).
            let got: Result<(*const fc::CharSet, *const fc::LangSet), Error> = {
                // SAFETY: `font_pattern` is the non-null render-prepared pattern.
                let fp = unsafe { &*font_pattern };
                match (
                    fp.get(fc::Property::Charset, 0),
                    fp.get(fc::Property::Lang, 0),
                ) {
                    (Ok(fc::Value::CharSet(cs)), Ok(fc::Value::LangSet(ls))) => Ok((cs, ls)),
                    (Err(e), _) | (_, Err(e)) => Err(Error::Fc(e)),
                    // A render-prepared pattern's `charset`/`lang` are always of
                    // the expected type; a mismatch is a fontconfig invariant break.
                    _ => Err(Error::Fc(fc::Error::FontconfigTypeMismatch)),
                }
            };
            let (charset, langset) = match got {
                Ok(v) => v,
                Err(e) => {
                    // SAFETY: `font_pattern` is owned here and not yet handed off.
                    unsafe { (*font_pattern).destroy() };
                    return Err(e);
                }
            };

            // Increment after we return
            // defer self.i += 1;
            self.i += 1;

            Ok(Some(DeferredFace {
                fc: Some(FontconfigData {
                    pattern: font_pattern,
                    charset,
                    langset,
                    variations: self.variations.clone(),
                }),
            }))
        }
    }
}

// ---------------------------------------------------------------------------
// WASM (web_canvas backend) discoverer — a functional NOOP.
//
// PORT NOTE: the reference `Discover` selector maps the `.web_canvas` backend to
// `void` ("no discovery" — see the `switch` comment above): the browser has no
// font-file discovery, fonts are named CSS strings resolved by the Canvas API.
// Rust cannot alias `Discover` to `void`/`()` because `CodepointResolver` and
// `SharedGridSet` name `Discover::init`/`.discover`/`.discover_fallback` and a
// `DiscoverIterator{next,deinit}` unconditionally (they are comptime-elided in
// the reference when `Discover == void`, but Rust type-checks them on every
// target). So on wasm `Discover` is a concrete NOOP discoverer whose iterators
// always yield `None` — behaviourally identical to the reference's "no discovery"
// (`self.discover` is always `null` in practice, so the resolver's discovery
// branch short-circuits before calling these). This is the faithful modelling of
// the `.web_canvas => void` arm under Rust's non-lazy type system.
#[cfg(target_arch = "wasm32")]
pub use wasm_noop::{DiscoverIterator, Error, Fontconfig};

#[cfg(target_arch = "wasm32")]
mod wasm_noop {
    use super::Descriptor;
    use crate::collection::Collection;
    use crate::deferred_face::DeferredFace;
    use crate::face::FaceError;
    use crate::library::Library;

    /// NOOP discoverer for the wasm `web_canvas` backend (`.web_canvas => void`).
    pub struct Fontconfig;

    /// The (never-produced) discovery error set, mirroring the macOS `Error`
    /// shape enough that `SharedGridSet`'s `From<discovery::Error>` composes. The
    /// NOOP never errors, so no variant is ever constructed.
    #[derive(Debug)]
    pub enum Error {
        Face(FaceError),
        Collection(crate::collection::Error),
    }

    impl From<FaceError> for Error {
        fn from(e: FaceError) -> Self {
            Error::Face(e)
        }
    }
    impl From<crate::collection::Error> for Error {
        fn from(e: crate::collection::Error) -> Self {
            Error::Collection(e)
        }
    }

    impl Fontconfig {
        // pub fn init(lib: Library) Fontconfig
        pub fn init(_lib: Library) -> Fontconfig {
            Fontconfig
        }

        // pub fn deinit(self: *Fontconfig) void
        pub fn deinit(&mut self) {}

        // pub fn discover(self, alloc, desc) !DiscoverIterator
        pub fn discover(&self, _desc: Descriptor) -> Result<DiscoverIterator, Error> {
            Ok(DiscoverIterator)
        }

        // pub fn discoverFallback(self, alloc, collection, desc) !DiscoverIterator
        pub fn discover_fallback(
            &self,
            _collection: *mut Collection,
            _desc: Descriptor,
        ) -> Result<DiscoverIterator, Error> {
            Ok(DiscoverIterator)
        }
    }

    /// NOOP discovery iterator: the browser exposes no enumerable font list, so
    /// this always yields `None` (no fallback faces).
    pub struct DiscoverIterator;

    impl DiscoverIterator {
        // pub fn deinit(self: *DiscoverIterator) void
        pub fn deinit(&mut self) {}

        // pub fn next(self: *DiscoverIterator) !?DeferredFace
        pub fn next(&mut self) -> Result<Option<DeferredFace>, Error> {
            Ok(None)
        }
    }
}

/// Descriptor is used to search for fonts. The only required field
/// is "family". The rest are ignored unless they're set to a non-zero
/// value.
// pub const Descriptor = struct { ... };
#[derive(Clone, Debug, Default)]
pub struct Descriptor {
    /// Font family to search for. This can be a fully qualified font
    /// name such as "Fira Code", "monospace", "serif", etc. Memory is
    /// owned by the caller and should be freed when this descriptor
    /// is no longer in use. The discovery structs will never store the
    /// descriptor.
    ///
    /// On systems that use fontconfig (Linux), this can be a full
    /// fontconfig pattern, such as "Fira Code-14:bold".
    // family: ?[:0]const u8 = null,
    pub family: Option<Vec<u8>>,

    /// Specific font style to search for. This will filter the style
    /// string the font advertises. The "bold/italic" booleans later in this
    /// struct filter by the style trait the font has, not the string, so
    /// these can be used in conjunction or not.
    // style: ?[:0]const u8 = null,
    pub style: Option<Vec<u8>>,

    /// A codepoint that this font must be able to render.
    // codepoint: u32 = 0,
    pub codepoint: u32,

    /// Font size in points that the font should support. For conversion
    /// to pixels, we will use 72 DPI for Mac and 96 DPI for everything else.
    /// (If pixel conversion is necessary, i.e. emoji fonts)
    // size: f32 = 0,
    pub size: f32,

    /// True if we want to search specifically for a font that supports
    /// specific styles.
    // bold: bool = false,
    pub bold: bool,
    // italic: bool = false,
    pub italic: bool,
    // monospace: bool = false,
    pub monospace: bool,

    /// Variation axes to apply to the font. This also impacts searching
    /// for fonts since fonts with the ability to set these variations
    /// will be preferred, but not guaranteed.
    // variations: []const Variation = &.{},
    pub variations: Vec<Variation>,
}

impl Descriptor {
    /// Hash the descriptor with the given hasher.
    // pub fn hash(self: Descriptor, hasher: anytype) void {
    //
    // (Named `hash` to match the reference; not the std `Hash::hash` trait method.)
    #[allow(clippy::should_implement_trait)]
    pub fn hash<H: core::hash::Hasher>(&self, hasher: &mut H) {
        use core::hash::Hash;
        // const autoHash = std.hash.autoHash;
        // const autoHashStrat = std.hash.autoHashStrat;
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
        // autoHash(hasher, self.variations.len);
        self.variations.len().hash(hasher);
        // for (self.variations) |variation| {
        for variation in &self.variations {
            // autoHash(hasher, variation.id);
            variation.id.to_u32().hash(hasher);

            // This is not correct, but we don't currently depend on the
            // hash value being different based on decimal values of variations.
            // autoHash(hasher, @as(i64, @intFromFloat(variation.value)));
            (variation.value as i64).hash(hasher);
        }
    }

    /// Returns a hash code that can be used to uniquely identify this
    /// action.
    // pub fn hashcode(self: Descriptor) u64 {
    pub fn hashcode(&self) -> u64 {
        // var hasher = std.hash.Wyhash.init(0);
        //
        // PORT NOTE: the reference seeds `std.hash.Wyhash`; rustty has no Wyhash
        // port in this tier, so the stdlib `DefaultHasher` is used. The descriptor
        // cache keyed by this hashcode is an internal cache key (never serialized
        // or compared cross-implementation), so the exact hash algorithm is not
        // observable; the ported tests only assert non-zero / inequality. A full
        // Wyhash port lands if a differential cache-key comparison ever needs it.
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        // self.hash(&hasher);
        self.hash(&mut hasher);
        // return hasher.final();
        core::hash::Hasher::finish(&hasher)
    }

    /// Deep copy of the struct. The given allocator is expected to
    /// be an arena allocator of some sort since the descriptor
    /// itself doesn't support fine-grained deallocation of fields.
    // pub fn clone(self: *const Descriptor, alloc: Allocator) !Descriptor {
    //
    // PORT NOTE: rustty uses owned `Vec` fields, so `clone` is the derived deep
    // clone; `alloc` is not threaded (fields free themselves).
    //
    // We can't do any errdefer cleanup in here. As documented we
    // expect the allocator to be an arena so any errors should be
    // cleaned up somewhere else.
    pub fn clone_desc(&self) -> Descriptor {
        // var copy = self.*;
        // copy.family = if (self.family) |src| try alloc.dupeZ(u8, src) else null;
        // copy.style = if (self.style) |src| try alloc.dupeZ(u8, src) else null;
        // copy.variations = try alloc.dupe(Variation, self.variations);
        // return copy;
        self.clone()
    }

    /// Convert to Fontconfig pattern to use for lookup. The pattern does
    /// not have defaults filled/substituted (Fontconfig thing) so callers
    /// must still do this.
    ///
    /// The returned raw pointer is owned by the caller (matching the reference's
    /// manually managed `*fontconfig.Pattern`); the caller must `destroy` it.
    // PORT NOTE: `discovery.zig:117-157` `toFcPattern`. `assert = quirks.inlineAssert`
    // (imported crate-wide as in `face/freetype.rs:41`); each `pat.add`/`cs.addChar`
    // is `assert`ed exactly as the reference. Gated to the fontconfig backend class
    // (`not(any(macos, wasm32))`) — the coretext path uses `to_coretext_descriptor`,
    // and wasm has no discovery.
    // pub fn toFcPattern(self: Descriptor) *fontconfig.Pattern {
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_arch = "wasm32")))]
    pub fn to_fc_pattern(&self) -> *mut crate::fc::Pattern {
        use crate::fc;
        use rustty_util::quirks::inlineAssert as assert;
        use std::ffi::CString;

        // const pat = fontconfig.Pattern.create();
        let pat_ptr = fc::Pattern::create();
        // SAFETY: `create` returns a freshly allocated, non-null pattern which this
        // function owns and hands back to the caller.
        let pat = unsafe { &mut *pat_ptr };

        // if (self.family) |family| {
        //     assert(pat.add(.family, .{ .string = family }, false));
        // }
        if let Some(family) = &self.family {
            // The reference `family` is a `[:0]const u8` (a C string); rustty stores
            // the raw bytes, so NUL-terminate here.
            //
            // PORT NOTE (task R13-P, N51): an interior NUL is REACHABLE — a
            // `font-family` config value carries one through, since the config
            // line reader keeps NUL bytes. The reference adds the `[:0]` pointer
            // to the pattern with `FcPatternAdd`, which reads it as a C string,
            // so fontconfig matches on the bytes before the NUL. This was an
            // `.expect`, which panicked.
            let end = family.iter().position(|&b| b == 0).unwrap_or(family.len());
            let family =
                CString::new(&family[..end]).expect("the bytes before the first NUL hold no NUL");
            assert(pat.add(fc::Property::Family, fc::Value::String(&family), false));
        }

        // if (self.style) |style| {
        //     assert(pat.add(.style, .{ .string = style }, false));
        // }
        if let Some(style) = &self.style {
            // (R13-P, N51 — see the `family` note just above.)
            let end = style.iter().position(|&b| b == 0).unwrap_or(style.len());
            let style =
                CString::new(&style[..end]).expect("the bytes before the first NUL hold no NUL");
            assert(pat.add(fc::Property::Style, fc::Value::String(&style), false));
        }

        // if (self.codepoint > 0) {
        //     const cs = fontconfig.CharSet.create();
        //     defer cs.destroy();
        //     assert(cs.addChar(self.codepoint));
        //     assert(pat.add(.charset, .{ .char_set = cs }, false));
        // }
        if self.codepoint > 0 {
            let cs_ptr = fc::CharSet::create();
            // SAFETY: `create` returns a freshly allocated, non-null char set.
            let cs = unsafe { &mut *cs_ptr };
            assert(cs.add_char(self.codepoint));
            // `add` (FcPatternAdd) copies the char set into pattern-owned memory, so
            // `cs` is destroyed below (the reference's `defer cs.destroy()`).
            assert(pat.add(
                fc::Property::Charset,
                fc::Value::CharSet(cs_ptr.cast_const()),
                false,
            ));
            cs.destroy();
        }

        // if (self.size > 0) assert(pat.add(
        //     .size,
        //     .{ .integer = @intFromFloat(@round(self.size)) },
        //     false,
        // ));
        if self.size > 0.0 {
            assert(pat.add(
                fc::Property::Size,
                fc::Value::Integer(self.size.round() as i32),
                false,
            ));
        }

        // if (self.bold) assert(pat.add(
        //     .weight,
        //     .{ .integer = @intFromEnum(fontconfig.Weight.bold) },
        //     false,
        // ));
        if self.bold {
            assert(pat.add(
                fc::Property::Weight,
                fc::Value::Integer(fc::Weight::Bold as i32),
                false,
            ));
        }

        // if (self.italic) assert(pat.add(
        //     .slant,
        //     .{ .integer = @intFromEnum(fontconfig.Slant.italic) },
        //     false,
        // ));
        if self.italic {
            assert(pat.add(
                fc::Property::Slant,
                fc::Value::Integer(fc::Slant::Italic as i32),
                false,
            ));
        }

        // For fontconfig, we always add monospace in the pattern. Since
        // fontconfig sorts by closeness to the pattern, this doesn't fully
        // exclude non-monospace but helps prefer it.
        // assert(pat.add(
        //     .spacing,
        //     .{ .integer = @intFromEnum(fontconfig.Spacing.mono) },
        //     false,
        // ));
        assert(pat.add(
            fc::Property::Spacing,
            fc::Value::Integer(fc::Spacing::Mono as i32),
            false,
        ));

        // return pat;
        pat_ptr
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
// PORT NOTE (DEFERRED — Fontconfig backend): `pub const Fontconfig = struct { ... }`
// is the Linux-default discoverer. It lands with the Fontconfig backend; recorded
// here verbatim:
//
// pub const Fontconfig = struct {
//     fc_config: *fontconfig.Config,
//
//     pub fn init(lib: Library) Fontconfig {
//         _ = lib;
//         // safe to call multiple times and concurrently
//         _ = fontconfig.init();
//         return .{ .fc_config = fontconfig.initLoadConfigAndFonts() };
//     }
//
//     pub fn deinit(self: *Fontconfig) void {
//         self.fc_config.destroy();
//     }
//
//     /// Discover fonts from a descriptor. This returns an iterator that can
//     /// be used to build up the deferred fonts.
//     pub fn discover(self, alloc, desc) !DiscoverIterator {
//         _ = alloc;
//         // Build our pattern that we'll search for
//         const pat = desc.toFcPattern();
//         errdefer pat.destroy();
//         assert(self.fc_config.substituteWithPat(pat, .pattern));
//         pat.defaultSubstitute();
//         // Search
//         const res = self.fc_config.fontSort(pat, false, null);
//         if (res.result != .match) return error.FontConfigFailed;
//         errdefer res.fs.destroy();
//         return .{
//             .config = self.fc_config,
//             .pattern = pat,
//             .set = res.fs,
//             .fonts = res.fs.fonts(),
//             .variations = desc.variations,
//             .i = 0,
//         };
//     }
//
//     pub fn discoverFallback(self, alloc, collection, desc) !DiscoverIterator {
//         _ = collection;
//         return try self.discover(alloc, desc);
//     }
//
//     pub const DiscoverIterator = struct {
//         config: *fontconfig.Config,
//         pattern: *fontconfig.Pattern,
//         set: *fontconfig.FontSet,
//         fonts: []*fontconfig.Pattern,
//         variations: []const Variation,
//         i: usize,
//
//         pub fn deinit(self: *DiscoverIterator) void {
//             self.set.destroy();
//             self.pattern.destroy();
//             self.* = undefined;
//         }
//
//         pub fn next(self: *DiscoverIterator) fontconfig.Error!?DeferredFace {
//             if (self.i >= self.fonts.len) return null;
//             // Get the copied pattern from our fontset that has the
//             // attributes configured for rendering.
//             const font_pattern = try self.config.fontRenderPrepare(
//                 self.pattern,
//                 self.fonts[self.i],
//             );
//             errdefer font_pattern.destroy();
//             // Increment after we return
//             defer self.i += 1;
//             return DeferredFace{
//                 .fc = .{
//                     .pattern = font_pattern,
//                     .charset = (try font_pattern.get(.charset, 0)).char_set,
//                     .langset = (try font_pattern.get(.lang, 0)).lang_set,
//                     .variations = self.variations,
//                 },
//             };
//         }
//     };
// };
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
pub use coretext_impl::{CoreText, DiscoverIterator, Error};

#[cfg(target_os = "macos")]
mod coretext_impl {
    use super::Descriptor;
    use crate::collection::{Collection, Index};
    use crate::deferred_face::{new_cfstring_bytes, CoreTextData, DeferredFace};
    use crate::face::coretext::copy_table_data;
    use crate::face::{Face, FaceError, Variation};
    use crate::library::Library;
    use crate::Style;
    use core::ffi::c_void;
    use core::ptr::{self, NonNull};
    use objc2_core_foundation::{
        kCFNull, kCFTypeArrayCallBacks, kCFTypeDictionaryKeyCallBacks,
        kCFTypeDictionaryValueCallBacks, CFArray, CFCharacterSet, CFDictionary,
        CFMutableDictionary, CFNumber, CFNumberType, CFRange, CFRetained, CFString, CFType,
    };
    use objc2_core_graphics::CGGlyph;
    use objc2_core_text::{
        kCTFontCharacterSetAttribute, kCTFontFamilyNameAttribute, kCTFontSizeAttribute,
        kCTFontStyleNameAttribute, kCTFontSymbolicTrait, kCTFontTraitsAttribute,
        kCTFontVariationAttribute, kCTFontVariationAxesAttribute,
        kCTFontVariationAxisDefaultValueKey, kCTFontVariationAxisIdentifierKey,
        kCTFontVariationAxisNameKey, CTFont, CTFontCollection, CTFontDescriptor,
        CTFontSymbolicTraits,
    };
    use rustty_font::opentype::head::Head;
    use rustty_font::opentype::os2::OS2;
    use rustty_util::quirks::inlineAssert as assert;

    /// The error set for CoreText discovery.
    // PORT NOTE: the reference errors are the inferred union of the CoreFoundation
    // allocation failures, `Allocator.Error`, `error.CodepointTooLarge` (utf8
    // encode), and the backend `Face` errors threaded through `getFace`. Modeled
    // as one enum with a `From<FaceError>` conversion so `?` composes them.
    #[derive(Debug)]
    pub enum Error {
        CoreTextFailure,
        OutOfMemory,
        CodepointTooLarge,
        Face(FaceError),
        Collection(crate::collection::Error),
    }

    impl From<FaceError> for Error {
        fn from(e: FaceError) -> Self {
            Error::Face(e)
        }
    }

    // `discoverCodepoint` threads `collection.getFace`'s error into the inferred
    // error set; here that backend-collection error composes via `?`.
    impl From<crate::collection::Error> for Error {
        fn from(e: crate::collection::Error) -> Self {
            Error::Collection(e)
        }
    }

    /// Raw CoreText entry points that objc2 0.3 wraps as non-null (they can
    /// legitimately return NULL, which the reference handles with `orelse`).
    // PORT NOTE: `CTFontCreateForString` is documented as returning the best
    // substitute font, but objc2's wrapper `CTFont::for_string` marks it non-null
    // and panics on NULL. The reference's `original.font.createForString(...) orelse
    // return null` relies on the nullable result, so the raw symbol is declared here
    // against the SAME framework objc2 links (R3 — a binding, not a reimplementation).
    // PORT NOTE: `CTFontCreateWithName` is the raw symbol behind the reference's
    // `pkg/macos` `Font.createWithName` (`orelse Allocator.Error.OutOfMemory`);
    // objc2's `CTFont::with_name` likewise panics on NULL, so the nullable result
    // needs the raw symbol (see `ct_font_create_with_name` below).
    mod sys {
        use super::*;
        #[link(name = "CoreText", kind = "framework")]
        extern "C-unwind" {
            pub fn CTFontCreateForString(
                current_font: &CTFont,
                string: &CFString,
                range: CFRange,
            ) -> *mut CTFont;
            pub fn CTFontCreateWithName(
                name: &CFString,
                size: f64,
                matrix: *const c_void,
            ) -> *mut CTFont;

            // PORT NOTE (parity S36-F3): the same treatment for the remaining
            // reference wrappers in this file that map a NULL CoreText result
            // to `Allocator.Error.OutOfMemory`. objc2's wrappers for these are
            // non-`Option` and `expect(..)` internally, so a NULL result
            // PANICKED where the reference returns an error the caller
            // propagates with `try`.
            pub fn CTFontCreateWithFontDescriptor(
                descriptor: &CTFontDescriptor,
                size: f64,
                matrix: *const c_void,
            ) -> *mut CTFont;
            pub fn CTFontDescriptorCreateWithAttributes(
                attributes: &CFDictionary,
            ) -> *mut CTFontDescriptor;
            pub fn CTFontDescriptorCreateCopyWithAttributes(
                original: &CTFontDescriptor,
                attributes: &CFDictionary,
            ) -> *mut CTFontDescriptor;
            pub fn CTFontCollectionCreateWithFontDescriptors(
                descriptors: &CFArray,
                options: *const c_void,
            ) -> *mut CTFontCollection;
        }
    }

    /// The reference `pkg/macos/text/font.zig` `Font.createWithName`:
    ///     pub fn createWithName(name: *foundation.String, size: f32) Allocator.Error!*Font {
    ///         return @as(?*Font, @ptrFromInt(@intFromPtr(c.CTFontCreateWithName(
    ///             @ptrCast(name), size, null,
    ///         )))) orelse Allocator.Error.OutOfMemory;
    ///     }
    // PORT NOTE: `matrix` is always `null` in the reference wrapper, so it is a
    // bare `*const c_void` here (never a real `CGAffineTransform`).
    fn ct_font_create_with_name(name: &CFString, size: f64) -> Result<CFRetained<CTFont>, Error> {
        // SAFETY: `name` is a live CFString; NULL matrix is documented-valid.
        let raw = unsafe { sys::CTFontCreateWithName(name, size, ptr::null()) };
        match NonNull::new(raw) {
            // SAFETY: CTFontCreateWithName returns a +1 ("Create rule") reference;
            // `CFRetained::from_raw` takes that ownership (releases on drop).
            Some(p) => Ok(unsafe { CFRetained::from_raw(p) }),
            // orelse Allocator.Error.OutOfMemory
            None => Err(Error::OutOfMemory),
        }
    }

    /// The reference `pkg/macos/text/font.zig` `Font.createWithFontDescriptor`:
    /// a NULL result is `Allocator.Error.OutOfMemory` (parity S36-F3). The
    /// reference wrapper always passes a NULL matrix.
    fn ct_font_create_with_font_descriptor(
        desc: &CTFontDescriptor,
        size: f64,
    ) -> Result<CFRetained<CTFont>, Error> {
        // SAFETY: `desc` is a live CTFontDescriptor; NULL matrix is documented-valid.
        let raw = unsafe { sys::CTFontCreateWithFontDescriptor(desc, size, ptr::null()) };
        match NonNull::new(raw) {
            // SAFETY: "Create rule" — the +1 reference is taken by `from_raw`.
            Some(p) => Ok(unsafe { CFRetained::from_raw(p) }),
            None => Err(Error::OutOfMemory),
        }
    }

    /// The reference `pkg/macos/text/font_descriptor.zig`
    /// `FontDescriptor.createWithAttributes` (parity S36-F3).
    fn ct_font_descriptor_create_with_attributes(
        attributes: &CFDictionary,
    ) -> Result<CFRetained<CTFontDescriptor>, Error> {
        // SAFETY: `attributes` is a live CFDictionary.
        let raw = unsafe { sys::CTFontDescriptorCreateWithAttributes(attributes) };
        match NonNull::new(raw) {
            // SAFETY: "Create rule" — the +1 reference is taken by `from_raw`.
            Some(p) => Ok(unsafe { CFRetained::from_raw(p) }),
            None => Err(Error::OutOfMemory),
        }
    }

    /// The reference `pkg/macos/text/font_descriptor.zig`
    /// `FontDescriptor.createCopyWithAttributes` (parity S36-F3).
    fn ct_font_descriptor_create_copy_with_attributes(
        original: &CTFontDescriptor,
        attributes: &CFDictionary,
    ) -> Result<CFRetained<CTFontDescriptor>, Error> {
        // SAFETY: both arguments are live CF objects.
        let raw = unsafe { sys::CTFontDescriptorCreateCopyWithAttributes(original, attributes) };
        match NonNull::new(raw) {
            // SAFETY: "Create rule" — the +1 reference is taken by `from_raw`.
            Some(p) => Ok(unsafe { CFRetained::from_raw(p) }),
            None => Err(Error::OutOfMemory),
        }
    }

    /// The reference `pkg/macos/text/font_collection.zig`
    /// `FontCollection.createWithFontDescriptors` (parity S36-F3). The
    /// reference wrapper always passes NULL options.
    fn ct_font_collection_create_with_font_descriptors(
        descriptors: &CFArray,
    ) -> Result<CFRetained<CTFontCollection>, Error> {
        // SAFETY: `descriptors` is a live CFArray; NULL options is documented-valid.
        let raw =
            unsafe { sys::CTFontCollectionCreateWithFontDescriptors(descriptors, ptr::null()) };
        match NonNull::new(raw) {
            // SAFETY: "Create rule" — the +1 reference is taken by `from_raw`.
            Some(p) => Ok(unsafe { CFRetained::from_raw(p) }),
            None => Err(Error::OutOfMemory),
        }
    }

    /// Create a fresh empty mutable `CFDictionary` with CoreFoundation type
    /// key/value callbacks (retain/release/equal/hash for CF objects).
    // PORT NOTE: `macos.foundation.MutableDictionary.create(0)`.
    fn new_mutable_dictionary() -> CFRetained<CFMutableDictionary> {
        unsafe {
            CFMutableDictionary::new(
                None,
                0,
                &kCFTypeDictionaryKeyCallBacks,
                &kCFTypeDictionaryValueCallBacks,
            )
        }
        .expect("CFDictionaryCreateMutable returned NULL")
    }

    /// Set `attrs[key] = value` where both are CoreFoundation objects.
    // PORT NOTE: `attrs.setValue(key, value)`.
    fn dict_set(attrs: &CFMutableDictionary, key: &CFString, value: &CFType) {
        unsafe {
            CFMutableDictionary::set_value(
                Some(attrs),
                key as *const CFString as *const c_void,
                value as *const CFType as *const c_void,
            );
        }
    }

    /// Look up `dict[key]` as a borrowed CF object (no retain), or `None`.
    // PORT NOTE: `dict.getValue(T, key)` — CFDictionaryGetValue returns a borrowed ref.
    unsafe fn dict_get(dict: &CFDictionary, key: *const c_void) -> Option<&CFType> {
        let p = unsafe { dict.value(key) };
        if p.is_null() {
            None
        } else {
            Some(unsafe { &*(p as *const CFType) })
        }
    }

    /// Read a CFNumber as an f64 (`getValue(.double, ...)`).
    fn number_as_f64(num: &CFNumber) -> f64 {
        let mut out: f64 = 0.0;
        unsafe {
            num.value(
                CFNumberType::DoubleType,
                &mut out as *mut f64 as *mut c_void,
            );
        }
        out
    }

    /// ASCII case-insensitive equality (`std.ascii.eqlIgnoreCase`).
    fn ascii_eql_ignore_case(a: &[u8], b: &[u8]) -> bool {
        a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.eq_ignore_ascii_case(y))
    }

    /// ASCII case-insensitive substring search (`std.ascii.indexOfIgnoreCase`).
    fn ascii_index_of_ignore_case(haystack: &[u8], needle: &[u8]) -> Option<usize> {
        if needle.is_empty() {
            return Some(0);
        }
        if needle.len() > haystack.len() {
            return None;
        }
        (0..=haystack.len() - needle.len())
            .find(|&i| haystack[i..i + needle.len()].eq_ignore_ascii_case(needle))
    }

    /// UTF-32 -> UTF-16 surrogate decomposition, matching CoreFoundation
    /// (`macos.foundation.stringGetSurrogatePairForLongCharacter`).
    fn string_get_surrogate_pair_for_long_character(
        character: u32,
        surrogates: &mut [u16; 2],
    ) -> bool {
        if (character > 0xFFFF) && (character < 0x110000) {
            let character = character - 0x10000;
            surrogates[0] = ((character >> 10) + 0xD800) as u16;
            surrogates[1] = ((character & 0x3FF) + 0xDC00) as u16;
            true
        } else {
            surrogates[0] = character as u16;
            false
        }
    }

    impl Descriptor {
        /// Convert to Core Text font descriptor to use for lookup or
        /// conversion to a specific font.
        // pub fn toCoreTextDescriptor(self: Descriptor) !*macos.text.FontDescriptor {
        pub fn to_core_text_descriptor(&self) -> Result<CFRetained<CTFontDescriptor>, Error> {
            // const attrs = try macos.foundation.MutableDictionary.create(0);
            let attrs = new_mutable_dictionary();
            // defer attrs.release(); — `attrs` is `CFRetained` and releases on drop.

            // Family
            // if (self.family) |family_bytes| {
            if let Some(family_bytes) = &self.family {
                // const family = try macos.foundation.String.createWithBytes(family_bytes, .utf8, false);
                let family = new_cfstring_bytes(family_bytes).ok_or(Error::OutOfMemory)?;
                // defer family.release();
                // attrs.setValue(macos.text.FontAttribute.family_name.key(), family);
                dict_set(&attrs, unsafe { kCTFontFamilyNameAttribute }, &family);
            }

            // Style
            // if (self.style) |style_bytes| {
            if let Some(style_bytes) = &self.style {
                // const style = try macos.foundation.String.createWithBytes(style_bytes, .utf8, false);
                let style = new_cfstring_bytes(style_bytes).ok_or(Error::OutOfMemory)?;
                // defer style.release();
                // attrs.setValue(macos.text.FontAttribute.style_name.key(), style);
                dict_set(&attrs, unsafe { kCTFontStyleNameAttribute }, &style);
            }

            // Codepoint support
            // if (self.codepoint > 0) {
            if self.codepoint > 0 {
                // const cs = try macos.foundation.CharacterSet.createWithCharactersInRange(.{
                //     .location = self.codepoint,
                //     .length = 1,
                // });
                let cs = unsafe {
                    CFCharacterSet::with_characters_in_range(
                        None,
                        CFRange {
                            location: self.codepoint as isize,
                            length: 1,
                        },
                    )
                }
                .ok_or(Error::OutOfMemory)?;
                // defer cs.release();
                // attrs.setValue(macos.text.FontAttribute.character_set.key(), cs);
                dict_set(&attrs, unsafe { kCTFontCharacterSetAttribute }, &cs);
            }

            // Set our size attribute if set
            // if (self.size > 0) {
            if self.size > 0.0 {
                // const size32: i32 = @intFromFloat(@round(self.size));
                let size32: i32 = self.size.round() as i32;
                // const size = try macos.foundation.Number.create(.sint32, &size32);
                let size = unsafe {
                    CFNumber::new(
                        None,
                        CFNumberType::SInt32Type,
                        &size32 as *const i32 as *const c_void,
                    )
                }
                .ok_or(Error::OutOfMemory)?;
                // defer size.release();
                // attrs.setValue(macos.text.FontAttribute.size.key(), size);
                dict_set(&attrs, unsafe { kCTFontSizeAttribute }, &size);
            }

            // Build our traits. If we set any, then we store it in the attributes
            // otherwise we do nothing. We determine this by setting up the packed
            // struct, converting to an int, and checking if it is non-zero.
            // const traits: macos.text.FontSymbolicTraits = .{
            //     .bold = self.bold, .italic = self.italic, .monospace = self.monospace,
            // };
            // const traits_cval: u32 = @bitCast(traits);
            let mut traits_cval: u32 = 0;
            if self.bold {
                traits_cval |= CTFontSymbolicTraits::TraitBold.0;
            }
            if self.italic {
                traits_cval |= CTFontSymbolicTraits::TraitItalic.0;
            }
            if self.monospace {
                traits_cval |= CTFontSymbolicTraits::TraitMonoSpace.0;
            }
            // if (traits_cval > 0) {
            if traits_cval > 0 {
                // Setting traits is a pain. We have to create a nested dictionary
                // of the symbolic traits value, and set that in our attributes.
                // const traits_num = try macos.foundation.Number.create(
                //     .sint32,
                //     @as(*const i32, @ptrCast(&traits_cval)),
                // );
                let traits_num = unsafe {
                    CFNumber::new(
                        None,
                        CFNumberType::SInt32Type,
                        &traits_cval as *const u32 as *const c_void,
                    )
                }
                .ok_or(Error::OutOfMemory)?;
                // defer traits_num.release();

                // const traits_dict = try macos.foundation.MutableDictionary.create(0);
                let traits_dict = new_mutable_dictionary();
                // defer traits_dict.release();
                // traits_dict.setValue(macos.text.FontTraitKey.symbolic.key(), traits_num);
                dict_set(&traits_dict, unsafe { kCTFontSymbolicTrait }, &traits_num);

                // attrs.setValue(macos.text.FontAttribute.traits.key(), traits_dict);
                dict_set(&attrs, unsafe { kCTFontTraitsAttribute }, &traits_dict);
            }

            // return try macos.text.FontDescriptor.createWithAttributes(@ptrCast(attrs));
            ct_font_descriptor_create_with_attributes(&attrs)
        }
    }

    /// The CoreText discoverer. Holds no state.
    // pub const CoreText = struct { ... };
    pub struct CoreText;

    impl CoreText {
        // pub fn init(lib: Library) CoreText {
        pub fn init(_lib: Library) -> CoreText {
            // _ = lib;
            // Required for the "interface" but does nothing for CoreText.
            CoreText
        }

        // pub fn deinit(self: *CoreText) void { _ = self; }
        pub fn deinit(&mut self) {}

        /// Warm up the system font registry.
        ///
        /// The first CoreText query in a process initializes the system font
        /// database, which takes multiple milliseconds, while subsequent
        /// queries are microseconds.
        // pub fn warmup() void {
        pub fn warmup() {
            // const name = macos.foundation.String.createWithBytes(
            //     "AppleColorEmoji",
            //     .utf8,
            //     false,
            // ) catch return;
            let Some(name) = new_cfstring_bytes(b"AppleColorEmoji") else {
                return;
            };
            // defer name.release(); — RAII (`CFRetained` drop).
            // const ct_font = macos.text.Font.createWithName(name, 12) catch return;
            let Ok(ct_font) = ct_font_create_with_name(&name, 12.0) else {
                return;
            };
            // ct_font.release(); — RAII (`CFRetained` drop).
            drop(ct_font);
        }

        /// Discover fonts from a descriptor. This returns an iterator that can
        /// be used to build up the deferred fonts.
        // pub fn discover(self: *const CoreText, alloc: Allocator, desc: Descriptor) !DiscoverIterator {
        //
        // PORT NOTE: the reference threads `alloc` for the matching-descriptor
        // slice; rustty uses a `Vec` (global allocator), so `alloc` is omitted.
        pub fn discover(&self, desc: Descriptor) -> Result<DiscoverIterator, Error> {
            // _ = self;

            // Build our pattern that we'll search for
            // const ct_desc = try desc.toCoreTextDescriptor();
            let ct_desc = desc.to_core_text_descriptor()?;
            // defer ct_desc.release();

            // Our descriptors have to be in an array
            // var ct_desc_arr = [_]*const macos.text.FontDescriptor{ct_desc};
            // const desc_arr = try macos.foundation.Array.create(macos.text.FontDescriptor, &ct_desc_arr);
            let mut ct_desc_arr: [*const c_void; 1] =
                [&*ct_desc as *const CTFontDescriptor as *const c_void];
            let desc_arr =
                unsafe { CFArray::new(None, ct_desc_arr.as_mut_ptr(), 1, &kCFTypeArrayCallBacks) }
                    .ok_or(Error::CoreTextFailure)?;
            // defer desc_arr.release();

            // Build our collection
            // const set = try macos.text.FontCollection.createWithFontDescriptors(desc_arr);
            let set = ct_font_collection_create_with_font_descriptors(&desc_arr)?;
            // defer set.release();
            // const list = set.createMatchingFontDescriptors();
            let list = unsafe { set.matching_font_descriptors() };
            // defer list.release();

            // Sort our descriptors
            // const zig_list = try copyMatchingDescriptors(alloc, list);
            let zig_list = copy_matching_descriptors(list);
            // errdefer alloc.free(zig_list);
            // sortMatchingDescriptors(&desc, zig_list);
            let zig_list = sort_matching_descriptors(&desc, zig_list);

            Ok(DiscoverIterator {
                list: zig_list,
                variations: desc.variations,
                i: 0,
            })
        }

        /// Discover a font by its exact name (family, full, or PostScript
        /// name). This is significantly faster than `discover` because it
        /// avoids the system-wide font matching that CTFontCollection does
        /// (which takes multiple milliseconds). This should be preferred
        /// when the desired font is known exactly, e.g. system fonts such
        /// as Apple Color Emoji.
        ///
        /// Returns null if no font with this exact family name exists;
        /// CoreText fallback fonts are never returned.
        // pub fn discoverExactFamily(self: *const CoreText, family: []const u8) !?DeferredFace {
        pub fn discover_exact_family(&self, family: &[u8]) -> Result<Option<DeferredFace>, Error> {
            // _ = self;

            // const family_str = try macos.foundation.String.createWithBytes(
            //     family,
            //     .utf8,
            //     false,
            // );
            let family_str = new_cfstring_bytes(family).ok_or(Error::OutOfMemory)?;
            // defer family_str.release(); — RAII (`CFRetained` drop).

            // Create our font. We need a size to initialize it so we use size
            // 12 but we will alter the size later (same as DiscoverIterator).
            // const ct_font = try macos.text.Font.createWithName(family_str, 12);
            let ct_font = ct_font_create_with_name(&family_str, 12.0)?;

            // CTFontCreateWithName never returns null: if the requested font
            // isn't installed it returns a substitute font. Verify we got
            // the family we asked for, otherwise report not found.
            // const found: bool = found: { ... };
            let found: bool = {
                // const actual = ct_font.copyFamilyName();
                // defer actual.release();
                let actual = unsafe { ct_font.family_name() };
                // var buf: [256]u8 = undefined;
                // const actual_slice = actual.cstring(&buf, .utf8) orelse
                //     break :found false;
                // break :found std.mem.eql(u8, actual_slice, family);
                //
                // S87-F2: the 256-byte buffer is part of the behaviour: a family
                // name that does not fit with its NUL (256 bytes or more) is
                // "not found". The port compared `to_string()` directly, so it
                // found such a family. CORRECTION (task T-F): the note here said
                // the buffer "and its too-small `orelse false` arm collapse into
                // the byte comparison".
                let mut buf = [0u8; 256];
                match crate::face::coretext::cstring_utf8(&actual, &mut buf) {
                    Some(actual_slice) => actual_slice == family,
                    None => false,
                }
            };
            if !found {
                // ct_font.release(); — RAII (`ct_font` drops on return).
                return Ok(None);
            }

            // return .{ .ct = .{ .font = ct_font, .variations = &.{} } };
            Ok(Some(DeferredFace {
                ct: Some(CoreTextData::new(ct_font, Vec::new())),
            }))
        }

        // pub fn discoverFallback(self, alloc, collection, desc) !DiscoverIterator {
        pub fn discover_fallback(
            &self,
            collection: *mut Collection,
            desc: Descriptor,
        ) -> Result<DiscoverIterator, Error> {
            // If we have a codepoint within the CJK unified ideographs block
            // then we fallback to macOS to find a font that supports it because
            // there isn't a better way manually with CoreText that I can find that
            // properly takes into account system locale.
            //
            // References:
            // - http://unicode.org/charts/PDF/U4E00.pdf
            // - https://chromium.googlesource.com/chromium/src/+/main/third_party/blink/renderer/platform/fonts/LocaleInFonts.md#unified-han-ideographs
            // if (desc.codepoint >= 0x4E00 and desc.codepoint <= 0x9FFF) han: {
            if desc.codepoint >= 0x4E00 && desc.codepoint <= 0x9FFF {
                // const han = try self.discoverCodepoint(collection, desc) orelse break :han;
                if let Some(han) = self.discover_codepoint(collection, &desc)? {
                    // This is silly but our discover iterator needs a slice so
                    // we allocate here. This isn't a performance bottleneck but
                    // this is something we can optimize very easily...
                    // const list = try alloc.alloc(*macos.text.FontDescriptor, 1);
                    // errdefer alloc.free(list);
                    // list[0] = han;
                    let list = vec![han];

                    return Ok(DiscoverIterator {
                        list,
                        variations: desc.variations,
                        i: 0,
                    });
                }
            }

            // const it = try self.discover(alloc, desc);
            let it = self.discover(desc.clone())?;

            // If our normal discovery doesn't find anything and we have a specific
            // codepoint, then fallback to using CTFontCreateForString to find a
            // matching font CoreText wants to use. See:
            // https://github.com/ghostty-org/ghostty/issues/2499
            // if (it.list.len == 0 and desc.codepoint > 0) codepoint: {
            if it.list.is_empty() && desc.codepoint > 0 {
                // const ct_desc = try self.discoverCodepoint(collection, desc) orelse break :codepoint;
                if let Some(ct_desc) = self.discover_codepoint(collection, &desc)? {
                    // const list = try alloc.alloc(*macos.text.FontDescriptor, 1);
                    // errdefer alloc.free(list);
                    // list[0] = ct_desc;
                    let list = vec![ct_desc];

                    return Ok(DiscoverIterator {
                        list,
                        variations: desc.variations,
                        i: 0,
                    });
                }
            }

            // return it;
            Ok(it)
        }

        /// Discover a font for a specific codepoint using the CoreText
        /// CTFontCreateForString API.
        // fn discoverCodepoint(self, collection: *Collection, desc: Descriptor) !?*macos.text.FontDescriptor {
        fn discover_codepoint(
            &self,
            collection: *mut Collection,
            desc: &Descriptor,
        ) -> Result<Option<CFRetained<CTFontDescriptor>>, Error> {
            // _ = self;

            // if (comptime options.backend.hasFreetype()) {
            //     // If we have freetype, we can't use CoreText to find a font
            //     // that supports a specific codepoint because we need to
            //     // have a CoreText font to be able to do so.
            //     return null;
            // }
            //
            // PORT NOTE: on the macOS default `coretext` backend `hasFreetype()`
            // is false, so this early-return does not apply and is a no-op here.

            // assert(desc.codepoint > 0);
            assert(desc.codepoint > 0);

            // SAFETY: `collection` is an externally-owned collection pointer.
            let collection = unsafe { &mut *collection };

            // Get our original font. This is dependent on the requested style
            // from the descriptor.
            // const original = original: {
            //     // In all the styles below, we try to match it but if we don't
            //     // we always fall back to some other option. The order matters
            //     // here.
            let original: *mut Face = {
                // if (desc.bold and desc.italic) {
                //     const entries = collection.faces.get(.bold_italic);
                //     if (entries.count() > 0) {
                //         break :original try collection.getFace(.{ .style = .bold_italic });
                //     }
                // }
                if desc.bold && desc.italic && collection.style_face_count(Style::BoldItalic) > 0 {
                    collection.get_face(Index::new(Style::BoldItalic, 0))?
                }
                // if (desc.bold) {
                //     const entries = collection.faces.get(.bold);
                //     if (entries.count() > 0) {
                //         break :original try collection.getFace(.{ .style = .bold });
                //     }
                // }
                else if desc.bold && collection.style_face_count(Style::Bold) > 0 {
                    collection.get_face(Index::new(Style::Bold, 0))?
                }
                // if (desc.italic) {
                //     const entries = collection.faces.get(.italic);
                //     if (entries.count() > 0) {
                //         break :original try collection.getFace(.{ .style = .italic });
                //     }
                // }
                else if desc.italic && collection.style_face_count(Style::Italic) > 0 {
                    collection.get_face(Index::new(Style::Italic, 0))?
                }
                // break :original try collection.getFace(.{ .style = .regular });
                else {
                    collection.get_face(Index::new(Style::Regular, 0))?
                }
            };

            // We need it in utf8 format
            // var buf: [4]u8 = undefined;
            // const len = try std.unicode.utf8Encode(@intCast(desc.codepoint), &buf);
            let ch = char::from_u32(desc.codepoint).ok_or(Error::CodepointTooLarge)?;
            let mut buf = [0u8; 4];
            let encoded = ch.encode_utf8(&mut buf);
            let len = encoded.len();

            // We need a CFString
            // const str = try macos.foundation.String.createWithBytes(buf[0..len], .utf8, false);
            let str = new_cfstring_bytes(&buf[0..len]).ok_or(Error::OutOfMemory)?;
            // defer str.release();

            // Get our range length for CTFontCreateForString. It looks like
            // the range uses UTF-16 codepoints and not UTF-32 codepoints.
            // const range_len: usize = range_len: {
            //     var unichars: [2]u16 = undefined;
            //     const pair = macos.foundation.stringGetSurrogatePairForLongCharacter(desc.codepoint, &unichars);
            //     break :range_len if (pair) 2 else 1;
            // };
            let range_len: isize = {
                let mut unichars: [u16; 2] = [0, 0];
                let pair =
                    string_get_surrogate_pair_for_long_character(desc.codepoint, &mut unichars);
                if pair {
                    2
                } else {
                    1
                }
            };

            // Get our font
            // const font = original.font.createForString(
            //     str,
            //     macos.foundation.Range.init(0, range_len),
            // ) orelse return null;
            let font_ptr = unsafe {
                sys::CTFontCreateForString(
                    &(*original).font,
                    &str,
                    CFRange {
                        location: 0,
                        length: range_len,
                    },
                )
            };
            let font: CFRetained<CTFont> = match NonNull::new(font_ptr) {
                Some(nn) => unsafe { CFRetained::from_raw(nn) },
                None => return Ok(None),
            };
            // defer font.release();

            // Do not allow the last resort font to go through. This is the
            // last font used by CoreText if it can't find anything else and
            // only contains replacement characters.
            // last_resort: {
            'last_resort: {
                // const name_str = font.copyPostScriptName();
                let name_str = unsafe { font.post_script_name() };
                // defer name_str.release();

                // If the name doesn't fit in our buffer, then it can't
                // be the last resort font so we break out.
                // var name_buf: [64]u8 = undefined;
                // const name: []const u8 = name_str.cstring(&name_buf, .utf8) orelse break :last_resort;
                let name = name_str.to_string();
                let name = name.as_bytes();
                // PORT NOTE: `cstring(&[64]u8, .utf8)` needs room for a trailing NUL,
                // so a name of >= 64 bytes returns null → break out (not last resort).
                if name.len() >= 64 {
                    break 'last_resort;
                }

                // If the name is "LastResort" then we don't want to use it.
                // if (std.mem.eql(u8, "LastResort", name)) return null;
                if b"LastResort" == name {
                    return Ok(None);
                }
            }

            // Get the descriptor
            // return font.copyDescriptor();
            Ok(Some(unsafe { font.font_descriptor() }))
        }
    }

    // fn copyMatchingDescriptors(alloc, list: *macos.foundation.Array) ![]*macos.text.FontDescriptor {
    fn copy_matching_descriptors(
        list: Option<CFRetained<CFArray>>,
    ) -> Vec<CFRetained<CTFontDescriptor>> {
        // var result = try alloc.alloc(*macos.text.FontDescriptor, list.getCount());
        // errdefer alloc.free(result);
        let mut result: Vec<CFRetained<CTFontDescriptor>> = Vec::new();
        // PORT NOTE: `createMatchingFontDescriptors` returns null when nothing
        // matches; the reference `list` is always non-null (empty array), so a
        // null result maps to the empty slice.
        let list = match list {
            Some(l) => l,
            None => return result,
        };
        // for (0..result.len) |i| {
        let count = CFArray::count(&list);
        for i in 0..count {
            // result[i] = list.getValueAtIndex(macos.text.FontDescriptor, i);
            let ptr = unsafe { CFArray::value_at_index(&list, i) };
            // We need to retain because once the list is freed it will
            // release all its members.
            // result[i].retain();
            let d: CFRetained<CTFontDescriptor> =
                unsafe { CFRetained::retain(NonNull::new_unchecked(ptr as *mut CTFontDescriptor)) };
            result.push(d);
        }
        // return result;
        result
    }

    // fn sortMatchingDescriptors(desc: *const Descriptor, list: []*macos.text.FontDescriptor) void {
    //
    // PORT NOTE: the reference sorts `list` in place with `std.mem.sortUnstable`,
    // whose comparator recomputes `Score.score` for each side on every comparison.
    // `score` is pure (deterministic), so rustty precomputes each descriptor's
    // score once and sorts the (score, descriptor) pairs — behavior-identical
    // ordering with O(n) scoring instead of O(n log n). Higher score sorts earlier.
    fn sort_matching_descriptors(
        desc: &Descriptor,
        list: Vec<CFRetained<CTFontDescriptor>>,
    ) -> Vec<CFRetained<CTFontDescriptor>> {
        let mut scored: Vec<(u32, CFRetained<CTFontDescriptor>)> = list
            .into_iter()
            .map(|d| (Score::score(desc, &d).int(), d))
            .collect();
        // fn lessThan(desc, lhs, rhs) bool {
        //     const lhs_score: Score = .score(desc, lhs);
        //     const rhs_score: Score = .score(desc, rhs);
        //     // Higher score is "less" (earlier)
        //     return lhs_score.int() > rhs_score.int();
        // }
        scored.sort_unstable_by(|a, b| b.0.cmp(&a.0));
        scored.into_iter().map(|(_, d)| d).collect()
    }

    /// We represent our sorting score as a packed struct so that we
    /// can compare scores numerically but build scores symbolically.
    ///
    /// Note that packed structs store their fields from least to most
    /// significant, so the fields here are defined in increasing order
    /// of precedence.
    // const Score = packed struct {
    #[derive(Default)]
    struct Score {
        // const Backing = @typeInfo(@This()).@"struct".backing_integer.?;
        //
        // PORT NOTE: the fields sum to u16 + u8 + 5 bools = 29 bits, so the packed
        // backing integer is `u32`. `int()` reproduces that little-endian bit
        // layout (first field = least-significant bits).
        /// Number of glyphs in the font, if two fonts have identical
        /// scores otherwise then we prefer the one with more glyphs.
        ///
        /// (Number of glyphs clamped at u16 intmax)
        // glyph_count: u16 = 0,
        glyph_count: u16,
        /// A fuzzy match on the style string, less important than
        /// an exact match, and less important than trait matches.
        // fuzzy_style: u8 = 0,
        fuzzy_style: u8,
        /// Whether the bold-ness of the font matches the descriptor.
        /// This is less important than italic because a font that's italic
        /// when it shouldn't be or not italic when it should be is a bigger
        /// problem (subjectively) than being the wrong weight.
        // bold: bool = false,
        bold: bool,
        /// Whether the italic-ness of the font matches the descriptor.
        /// This is less important than an exact match on the style string
        /// because we want users to be allowed to override trait matching
        /// for the bold/italic/bold italic styles if they want.
        // italic: bool = false,
        italic: bool,
        /// An exact (case-insensitive) match on the style string.
        // exact_style: bool = false,
        exact_style: bool,
        /// Whether the font is monospace, this is more important than any of
        /// the other fields unless we're looking for a specific codepoint,
        /// in which case that is the most important thing.
        // monospace: bool = false,
        monospace: bool,
        /// If we're looking for a codepoint, whether this font has it.
        // codepoint: bool = false,
        codepoint: bool,
    }

    impl Score {
        // pub fn int(self: Score) Backing {
        //     return @bitCast(self);
        // }
        fn int(&self) -> u32 {
            (self.glyph_count as u32)
                | ((self.fuzzy_style as u32) << 16)
                | ((self.bold as u32) << 24)
                | ((self.italic as u32) << 25)
                | ((self.exact_style as u32) << 26)
                | ((self.monospace as u32) << 27)
                | ((self.codepoint as u32) << 28)
        }

        // fn score(desc: *const Descriptor, ct_desc: *const macos.text.FontDescriptor) Score {
        // (Named `score` to match the reference associated fn on `Score`; the
        // clippy `self_named_constructors` lint fires on the type/fn name match,
        // but this is not a constructor — it computes a score, not a `Score::new`.)
        #[allow(clippy::self_named_constructors)]
        fn score(desc: &Descriptor, ct_desc: &CTFontDescriptor) -> Score {
            // var self: Score = .{};
            let mut this = Score::default();

            // We always load the font if we can since some things can only be
            // inspected on the font itself. Fonts that can't be loaded score
            // 0 automatically because we don't want a font we can't load.
            // const font: *macos.text.Font = macos.text.Font.createWithFontDescriptor(ct_desc, 12) catch return self;
            //
            // PORT NOTE (parity S36-F3): objc2's `with_font_descriptor` is
            // non-`Option` and PANICS on a NULL result; the reference returns
            // `error.OutOfMemory` and this site catches it into a 0 score. The
            // raw symbol restores that arm.
            let font = match ct_font_create_with_font_descriptor(ct_desc, 12.0) {
                Ok(f) => f,
                Err(_) => return this,
            };
            // defer font.release();

            // We prefer fonts with more glyphs, all else being equal.
            {
                // const Type = @TypeOf(self.glyph_count); // u16
                // self.glyph_count = std.math.cast(Type, font.getGlyphCount()) orelse std.math.maxInt(Type);
                let gc = unsafe { font.glyph_count() };
                this.glyph_count = u16::try_from(gc).unwrap_or(u16::MAX);
            }

            // If we're searching for a codepoint, then we
            // prioritize fonts that have that codepoint.
            // if (desc.codepoint > 0) {
            if desc.codepoint > 0 {
                // Turn UTF-32 into UTF-16 for CT API
                // var unichars: [2]u16 = undefined;
                // const pair = macos.foundation.stringGetSurrogatePairForLongCharacter(desc.codepoint, &unichars);
                // const len: usize = if (pair) 2 else 1;
                let mut unichars: [u16; 2] = [0, 0];
                let pair =
                    string_get_surrogate_pair_for_long_character(desc.codepoint, &mut unichars);
                let len: usize = if pair { 2 } else { 1 };

                // Get our glyphs
                // var glyphs = [2]macos.graphics.Glyph{ 0, 0 };
                // self.codepoint = font.getGlyphsForCharacters(unichars[0..len], glyphs[0..len]);
                let mut glyphs = [0 as CGGlyph; 2];
                this.codepoint = unsafe {
                    font.glyphs_for_characters(
                        NonNull::new(unichars.as_mut_ptr()).unwrap(),
                        NonNull::new(glyphs.as_mut_ptr()).unwrap(),
                        len as isize,
                    )
                };
            }

            // Get our symbolic traits for the descriptor so we can
            // compare boolean attributes like bold, monospace, etc.
            // const symbolic_traits: macos.text.FontSymbolicTraits = traits: {
            //     const traits = ct_desc.copyAttribute(.traits) orelse break :traits .{};
            //     defer traits.release();
            //     const key = macos.text.FontTraitKey.symbolic.key();
            //     const symbolic = traits.getValue(macos.foundation.Number, key) orelse break :traits .{};
            //     break :traits macos.text.FontSymbolicTraits.init(symbolic);
            // };
            let symbolic_traits: CTFontSymbolicTraits = 'traits: {
                let traits = match unsafe { ct_desc.attribute(kCTFontTraitsAttribute) } {
                    Some(t) => t,
                    None => break 'traits CTFontSymbolicTraits(0),
                };
                let traits = match traits.downcast_ref::<CFDictionary>() {
                    Some(d) => d,
                    None => break 'traits CTFontSymbolicTraits(0),
                };
                let key = unsafe { kCTFontSymbolicTrait };
                let symbolic =
                    match unsafe { dict_get(traits, key as *const CFString as *const c_void) }
                        .and_then(|v| v.downcast_ref::<CFNumber>())
                    {
                        Some(n) => n,
                        None => break 'traits CTFontSymbolicTraits(0),
                    };
                // FontSymbolicTraits.init(symbolic) — read the int value.
                let mut raw: i32 = 0;
                unsafe {
                    symbolic.value(CFNumberType::IntType, &mut raw as *mut i32 as *mut c_void);
                }
                break 'traits CTFontSymbolicTraits(raw as u32);
            };

            // self.monospace = symbolic_traits.monospace;
            this.monospace = symbolic_traits.0 & CTFontSymbolicTraits::TraitMonoSpace.0 != 0;

            // We try to derived data from the font itself, which is generally
            // more reliable than only using the symbolic traits for this.
            // const is_bold: bool, const is_italic: bool = derived: {
            let (is_bold, is_italic): (bool, bool) = 'derived: {
                // We start with initial guesses based on the symbolic traits,
                // but refine these with more information if we can get it.
                // var is_italic = symbolic_traits.italic;
                // var is_bold = symbolic_traits.bold;
                let mut is_italic = symbolic_traits.0 & CTFontSymbolicTraits::TraitItalic.0 != 0;
                let mut is_bold = symbolic_traits.0 & CTFontSymbolicTraits::TraitBold.0 != 0;

                // Read the 'head' table out of the font data if it's available.
                // if (head: {
                //     const tag = macos.text.FontTableTag.init("head");
                //     const data = font.copyTable(tag) orelse break :head null;
                //     defer data.release();
                //     const ptr = data.getPointer();
                //     const len = data.getLength();
                //     break :head opentype.Head.init(ptr[0..len]) catch |err| {
                //         log.warn("error parsing head table: {}", .{err});
                //         break :head null;
                //     };
                // }) |head_| {
                //     const head: opentype.Head = head_;
                //     is_bold = is_bold or (head.macStyle & 1 == 1);
                //     is_italic = is_italic or (head.macStyle & 2 == 2);
                // }
                let head_: Option<Head> = match copy_table_data(&font, b"head") {
                    None => None,
                    // break :head opentype.Head.init(ptr[0..len]) catch |err| {
                    //     log.warn("error parsing head table: {}", .{err});
                    //     break :head null;
                    // };
                    //
                    // `Head::init` returns `Result`, so this IS a reachable
                    // failure arm — the `.ok()` that used to stand here silently
                    // discarded both the error and the reference's diagnostic.
                    Some(data) => {
                        crate::discovery::parse_head_table(unsafe { data.as_bytes_unchecked() })
                    }
                };
                if let Some(head) = head_ {
                    is_bold = is_bold || (head.macStyle & 1 == 1);
                    is_italic = is_italic || (head.macStyle & 2 == 2);
                }

                // Read the 'OS/2' table out of the font data if it's available.
                // if (os2: { ... opentype.OS2.init ... }) |os2| {
                //     is_bold = is_bold or os2.fsSelection.bold;
                //     is_italic = is_italic or os2.fsSelection.italic;
                // }
                let os2_: Option<OS2> = match copy_table_data(&font, b"OS/2") {
                    None => None,
                    // break :os2 opentype.OS2.init(ptr[0..len]) catch |err| {
                    //     log.warn("error parsing OS/2 table: {}", .{err});
                    //     break :os2 null;
                    // };
                    //
                    // Same shape as the `head` arm above: a reachable failure
                    // arm, not a success path.
                    Some(data) => {
                        crate::discovery::parse_os2_table(unsafe { data.as_bytes_unchecked() })
                    }
                };
                if let Some(os2) = os2_ {
                    is_bold = is_bold || os2.fsSelection.bold;
                    is_italic = is_italic || os2.fsSelection.italic;
                }

                // Check if we have variation axes in our descriptor, if we
                // do then we can derive weight italic-ness or both from them.
                // if (font.copyAttribute(.variation_axes)) |axes| variations: {
                if let Some(axes) = unsafe { font.attribute(kCTFontVariationAxesAttribute) } {
                    'variations: {
                        // defer axes.release();
                        let axes = match axes.downcast_ref::<CFArray>() {
                            Some(a) => a,
                            None => break 'variations,
                        };

                        // Copy the variation values for this instance of the font.
                        // if there are none then we just break out immediately.
                        // const values: *macos.foundation.Dictionary = font.copyAttribute(.variation) orelse break :variations;
                        // defer values.release();
                        let values_ret = match unsafe { font.attribute(kCTFontVariationAttribute) }
                        {
                            Some(v) => v,
                            None => break 'variations,
                        };
                        let values = match values_ret.downcast_ref::<CFDictionary>() {
                            Some(d) => d,
                            None => break 'variations,
                        };

                        // var buf: [1024]u8 = undefined;

                        // If we see the 'ital' value then we ignore 'slnt'.
                        // var ital_seen = false;
                        let mut ital_seen = false;

                        // const len = axes.getCount();
                        let len = CFArray::count(axes);
                        // for (0..len) |i| {
                        for i in 0..len {
                            // const dict = axes.getValueAtIndex(macos.foundation.Dictionary, i);
                            let dict_ptr = unsafe { CFArray::value_at_index(axes, i) };
                            if dict_ptr.is_null() {
                                continue;
                            }
                            let dict: &CFDictionary =
                                unsafe { &*(dict_ptr as *const CFDictionary) };
                            // const Key = macos.text.FontVariationAxisKey;
                            // const cf_id = dict.getValue(Key.identifier.Value(), Key.identifier.key()).?;
                            // const cf_name = dict.getValue(Key.name.Value(), Key.name.key()).?;
                            // const cf_def = dict.getValue(Key.default_value.Value(), Key.default_value.key()).?;
                            //
                            // PORT NOTE (R7, checked under R13 by task R13-C, inventory
                            // M87): the three `.expect`s below are invariant checks,
                            // not reachable reference defects. CoreText builds each
                            // axis dictionary itself and always supplies all three
                            // keys. Measured on macOS 26.5.1, 2026-09-22: all 113 axes
                            // of the 55 variable fonts among the 528 installed
                            // descriptors have them; and for a crafted font whose
                            // `fvar` axis names a missing `name` record, whose `name`
                            // table is removed, or whose axis tag is non-printable,
                            // CoreText still supplies the name (the tag, or an empty
                            // string) as a CFString and the identifier and default as
                            // CFNumbers. A font file cannot remove them.
                            let cf_id = unsafe {
                                dict_get(
                                    dict,
                                    kCTFontVariationAxisIdentifierKey as *const CFString
                                        as *const c_void,
                                )
                            }
                            .expect("variation axis missing identifier");
                            let cf_name = unsafe {
                                dict_get(
                                    dict,
                                    kCTFontVariationAxisNameKey as *const CFString as *const c_void,
                                )
                            }
                            .and_then(|v| v.downcast_ref::<CFString>())
                            .expect("variation axis missing name");
                            let cf_def = unsafe {
                                dict_get(
                                    dict,
                                    kCTFontVariationAxisDefaultValueKey as *const CFString
                                        as *const c_void,
                                )
                            }
                            .and_then(|v| v.downcast_ref::<CFNumber>())
                            .expect("variation axis missing default value");

                            // const name_str = cf_name.cstring(&buf, .utf8) orelse "";
                            let name_string = cf_name.to_string();
                            // PORT NOTE: `cstring(&[1024]u8, .utf8)` returns "" when
                            // the name doesn't fit its buffer (needs a NUL byte).
                            let name_str: &[u8] = if name_string.len() < 1024 {
                                name_string.as_bytes()
                            } else {
                                b""
                            };

                            // Default value
                            // var def: f64 = 0;
                            // _ = cf_def.getValue(.double, &def);
                            let def: f64 = number_as_f64(cf_def);
                            // Value in this font
                            // var val: f64 = def;
                            // if (values.getValue(macos.foundation.Number, cf_id)) |cf_val| _ = cf_val.getValue(.double, &val);
                            let mut val: f64 = def;
                            if let Some(cf_val) =
                                unsafe { dict_get(values, cf_id as *const CFType as *const c_void) }
                                    .and_then(|v| v.downcast_ref::<CFNumber>())
                            {
                                val = number_as_f64(cf_val);
                            }

                            // if (std.mem.eql(u8, "wght", name_str)) {
                            if name_str == b"wght" {
                                // Somewhat subjective threshold, we consider fonts
                                // bold if they have a 'wght' set greater than 600.
                                is_bold = val > 600.0;
                                continue;
                            }
                            // if (std.mem.eql(u8, "ital", name_str)) {
                            if name_str == b"ital" {
                                is_italic = val > 0.5;
                                ital_seen = true;
                                continue;
                            }
                            // if (!ital_seen and std.mem.eql(u8, "slnt", name_str)) {
                            if !ital_seen && name_str == b"slnt" {
                                // Arbitrary threshold of anything more than a 5
                                // degree clockwise slant is considered italic.
                                is_italic = val <= -5.0;
                                continue;
                            }
                        }
                    }
                }

                // break :derived .{ is_bold, is_italic };
                break 'derived (is_bold, is_italic);
            };

            // self.bold = desc.bold == is_bold;
            // self.italic = desc.italic == is_italic;
            this.bold = desc.bold == is_bold;
            this.italic = desc.italic == is_italic;

            // Get the style string from the font.
            // var style_str_buf: [128]u8 = undefined;
            // const style_str: []const u8 = style_str: {
            //     const style = ct_desc.copyAttribute(.style_name) orelse break :style_str "";
            //     defer style.release();
            //     break :style_str style.cstring(&style_str_buf, .utf8) orelse "";
            // };
            let style_string: String = match unsafe { ct_desc.attribute(kCTFontStyleNameAttribute) }
                .and_then(|v| v.downcast::<CFString>().ok())
            {
                Some(s) => s.to_string(),
                None => String::new(),
            };
            // PORT NOTE: `cstring(&[128]u8, .utf8)` returns "" when the style name
            // doesn't fit its buffer (needs a NUL byte).
            let style_str: &[u8] = if style_string.len() < 128 {
                style_string.as_bytes()
            } else {
                b""
            };

            // The first string in this slice will be used for the exact match,
            // and for the fuzzy match, all matching substrings will increase
            // the rank.
            // const desired_styles: []const [:0]const u8 = desired: {
            //     if (desc.style) |s| break :desired &.{s};
            //
            //     // If we don't have an explicitly desired style name, we base
            //     // it on the bold and italic properties, this isn't ideal since
            //     // fonts may use style names other than these, but it helps in
            //     // some edge cases.
            //     if (desc.bold) {
            //         if (desc.italic) break :desired &.{ "bold italic", "bold", "italic", "oblique" };
            //         break :desired &.{ "bold", "upright" };
            //     } else if (desc.italic) {
            //         break :desired &.{ "italic", "regular", "oblique" };
            //     }
            //     break :desired &.{ "regular", "upright" };
            // };
            let desired_styles: Vec<&[u8]> = if let Some(s) = &desc.style {
                vec![s.as_slice()]
            }
            // If we don't have an explicitly desired style name, we base
            // it on the bold and italic properties, this isn't ideal since
            // fonts may use style names other than these, but it helps in
            // some edge cases.
            else if desc.bold {
                if desc.italic {
                    vec![
                        &b"bold italic"[..],
                        &b"bold"[..],
                        &b"italic"[..],
                        &b"oblique"[..],
                    ]
                } else {
                    vec![&b"bold"[..], &b"upright"[..]]
                }
            } else if desc.italic {
                vec![&b"italic"[..], &b"regular"[..], &b"oblique"[..]]
            } else {
                vec![&b"regular"[..], &b"upright"[..]]
            };

            // self.exact_style = std.ascii.eqlIgnoreCase(style_str, desired_styles[0]);
            this.exact_style = ascii_eql_ignore_case(style_str, desired_styles[0]);
            // Our "fuzzy match" score is 0 if the desired style isn't present
            // in the string, otherwise we give higher priority for styles that
            // have fewer characters not in the desired_styles list.
            // const fuzzy_type = @TypeOf(self.fuzzy_style); // u8
            // self.fuzzy_style = @intCast(style_str.len);
            this.fuzzy_style = style_str.len() as u8;
            // for (desired_styles) |s| {
            for s in &desired_styles {
                // if (std.ascii.indexOfIgnoreCase(style_str, s) != null) {
                if ascii_index_of_ignore_case(style_str, s).is_some() {
                    // self.fuzzy_style -|= @intCast(s.len);
                    this.fuzzy_style = this.fuzzy_style.saturating_sub(s.len() as u8);
                }
            }
            // self.fuzzy_style = std.math.maxInt(fuzzy_type) -| self.fuzzy_style;
            this.fuzzy_style = u8::MAX.saturating_sub(this.fuzzy_style);

            // return self;
            this
        }
    }

    /// Iterator over the matching descriptors, yielding `DeferredFace`s.
    // pub const DiscoverIterator = struct {
    pub struct DiscoverIterator {
        // alloc: Allocator,  (rustty uses a `Vec` — global allocator; no `alloc`.)
        // list: []const *macos.text.FontDescriptor,
        list: Vec<CFRetained<CTFontDescriptor>>,
        // variations: []const Variation,
        variations: Vec<Variation>,
        // i: usize,
        i: usize,
    }

    impl DiscoverIterator {
        // pub fn deinit(self: *DiscoverIterator) void {
        //     for (self.list) |desc| desc.release();
        //     self.alloc.free(self.list);
        //     self.* = undefined;
        // }
        //
        // PORT NOTE: RAII — each `CFRetained<CTFontDescriptor>` releases on drop and
        // the `Vec` frees its backing store; `clear()` drops them now, matching the
        // reference's explicit release + free.
        pub fn deinit(&mut self) {
            self.list.clear();
        }

        // pub fn next(self: *DiscoverIterator) !?DeferredFace {
        // (Named `next` to match the reference iterator; not `Iterator::next`,
        // whose signature — no `Result`, no external `deinit` — doesn't fit.)
        #[allow(clippy::should_implement_trait)]
        pub fn next(&mut self) -> Result<Option<DeferredFace>, Error> {
            // if (self.i >= self.list.len) return null;
            if self.i >= self.list.len() {
                return Ok(None);
            }

            // Get our descriptor. We need to remove the character set
            // limitation because we may have used that to filter but we
            // don't want it anymore because it'll restrict the characters
            // available.
            // const desc = desc: {
            //     // We create a copy, overwriting the character set attribute.
            //     const attrs = try macos.foundation.MutableDictionary.create(0);
            //     defer attrs.release();
            //     attrs.setValue(macos.text.FontAttribute.character_set.key(), macos.c.kCFNull);
            //     break :desc try macos.text.FontDescriptor.createCopyWithAttributes(self.list[self.i], @ptrCast(attrs));
            // };
            // defer desc.release();
            let desc = {
                let attrs = new_mutable_dictionary();
                let null_val = unsafe { kCFNull }.expect("kCFNull is NULL");
                unsafe {
                    CFMutableDictionary::set_value(
                        Some(&attrs),
                        kCTFontCharacterSetAttribute as *const CFString as *const c_void,
                        null_val as *const _ as *const c_void,
                    );
                }
                ct_font_descriptor_create_copy_with_attributes(&self.list[self.i], &attrs)?
            };

            // Create our font. We need a size to initialize it so we use size
            // 12 but we will alter the size later.
            // const font = try macos.text.Font.createWithFontDescriptor(desc, 12);
            // errdefer font.release();
            let font: CFRetained<CTFont> = ct_font_create_with_font_descriptor(&desc, 12.0)?;

            // Increment after we return
            // defer self.i += 1;
            self.i += 1;

            // return DeferredFace{ .ct = .{ .font = font, .variations = self.variations } };
            Ok(Some(DeferredFace {
                ct: Some(CoreTextData::new(font, self.variations.clone())),
            }))
        }
    }
}

// ---------------------------------------------------------------------------
// Windows (`freetype_windows`) FreeType directory-scan discoverer. LIVE on
// `target_os = "windows"`, where `Discover` IS `Windows` and `Face` IS
// `face::freetype::Face`. Ported by WS20 (2026-07-30) from
// `discovery.zig:890-1147`; it was previously a verbatim reference comment.
// ---------------------------------------------------------------------------

/// Windows font discovery. Enumerates font files in the system and
/// per-user font directories and matches them to a descriptor via
/// FreeType's family_name field (with a fallback to the SFNT name
/// table when family_name is missing).
///
/// No external service is used; each discover() call walks the
/// directories, opening candidate files with FreeType only as needed.
/// For typical Windows installations (~300 fonts) a name query is in
/// the tens of milliseconds. A codepoint fallback query may be
/// noticeably slower because every candidate has to be opened to
/// probe its CMap.
#[cfg(all(target_os = "windows", not(target_arch = "wasm32")))]
pub use windows_impl::{DiscoverIterator, Error, Windows};

#[cfg(all(target_os = "windows", not(target_arch = "wasm32")))]
mod windows_impl {
    use super::Descriptor;
    use crate::collection::Collection;
    use crate::deferred_face::{DeferredFace, WindowsData};
    use crate::face::{DesiredSize, Face, FaceError, Options, Variation, DEFAULT_DPI};
    use crate::library::Library;
    use crate::Presentation;
    use std::ffi::{CString, OsStr, OsString};

    /// The error set for Windows discovery.
    // PORT NOTE: the reference's inferred error union here is `Allocator.Error`
    // (`alloc.dupeZ` in `makeDeferred`, `allocPrintSentinel` in the two path
    // builders — the latter `catch null`s, so it does not escape) plus the backend
    // `Face`/`Collection` errors threaded through `discoverFallback`. Modeled as one
    // enum with `From` conversions so `?` composes them — the SAME shape as the
    // macOS `coretext_impl::Error` and the Linux `fontconfig_impl::Error`, so
    // `SharedGridSet`'s `Discovery(discovery::Error)` variant composes identically
    // on all three targets.
    #[derive(Debug)]
    pub enum Error {
        /// `Allocator.Error` — `makeDeferred`'s `alloc.dupeZ(u8, full_path)`.
        OutOfMemory,
        Face(FaceError),
        Collection(crate::collection::Error),
    }

    impl From<FaceError> for Error {
        fn from(e: FaceError) -> Self {
            Error::Face(e)
        }
    }

    impl From<crate::collection::Error> for Error {
        fn from(e: crate::collection::Error) -> Self {
            Error::Collection(e)
        }
    }

    /// The Windows discoverer (the `freetype_windows` backend).
    // pub const Windows = struct { lib: Library };
    pub struct Windows {
        // lib: Library,
        lib: Library,
    }

    impl Windows {
        // pub fn init(lib: Library) Windows { return .{ .lib = lib }; }
        pub fn init(lib: Library) -> Windows {
            Windows { lib }
        }

        // pub fn deinit(self: *Windows) void { _ = self; }
        pub fn deinit(&mut self) {}

        /// Discover fonts from a descriptor. This returns an iterator that can
        /// be used to build up the deferred fonts.
        // pub fn discover(self: *const Windows, alloc: Allocator, desc: Descriptor) !DiscoverIterator
        //
        // PORT NOTE: the reference threads `alloc`; rustty uses the global
        // allocator, so `alloc` is omitted (matching `CoreText`/`Fontconfig`).
        pub fn discover(&self, desc: Descriptor) -> Result<DiscoverIterator, Error> {
            // return .{
            //     .alloc = alloc, .lib = self.lib, .desc = desc,
            //     .variations = desc.variations, .state = .system,
            //     .dir = null, .iter = null, .system_path = null, .user_path = null,
            // };
            Ok(DiscoverIterator {
                lib: self.lib,
                variations: desc.variations.clone(),
                desc,
                state: State::System,
                dir: None,
                system_path: None,
                user_path: None,
            })
        }

        // pub fn discoverFallback(self, alloc, collection, desc) !DiscoverIterator {
        //     _ = collection;
        //     return self.discover(alloc, desc);
        // }
        pub fn discover_fallback(
            &self,
            _collection: *mut Collection,
            desc: Descriptor,
        ) -> Result<DiscoverIterator, Error> {
            self.discover(desc)
        }
    }

    // const State = enum { system, user, done };
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum State {
        System,
        User,
        Done,
    }

    /// The Windows discovery iterator.
    pub struct DiscoverIterator {
        // alloc: Allocator,  — the global allocator; no field.
        // lib: Library,
        lib: Library,
        // desc: Descriptor,
        desc: Descriptor,
        // variations: []const Variation,
        variations: Vec<Variation>,
        // state: State,
        state: State,
        // dir: ?std.Io.Dir,
        // iter: ?std.Io.Dir.Iterator,
        //
        // PORT NOTE: `std::fs::ReadDir` IS the open directory handle AND its lazy
        // iterator, so the reference's two fields collapse into one. Closing is
        // dropping it (the reference's `d.close(global.io())`).
        dir: Option<std::fs::ReadDir>,
        // system_path: ?[:0]const u8,
        //
        // PORT NOTE (byte fidelity): a Windows directory path is `OsString`
        // (WTF-8), NOT necessarily valid UTF-8, so it is kept as `OsString` and
        // concatenated at the byte level — never through a lossy `String`.
        system_path: Option<OsString>,
        // user_path: ?[:0]const u8,
        user_path: Option<OsString>,
    }

    impl DiscoverIterator {
        // pub fn deinit(self: *DiscoverIterator) void {
        //     if (self.dir) |*d| d.close(global.io());
        //     if (self.system_path) |p| self.alloc.free(p);
        //     if (self.user_path) |p| self.alloc.free(p);
        //     self.* = undefined;
        // }
        pub fn deinit(&mut self) {
            // All three are owning Rust values: dropping them IS the close/free.
            self.dir = None;
            self.system_path = None;
            self.user_path = None;
        }

        // pub fn next(self: *DiscoverIterator) !?DeferredFace {
        // (Named `next` to match the reference iterator; not `Iterator::next`,
        // whose signature — no `Result`, no external `deinit` — doesn't fit.)
        #[allow(clippy::should_implement_trait)]
        pub fn next(&mut self) -> Result<Option<DeferredFace>, Error> {
            // while (true) {
            loop {
                // Ensure we have a directory iterator for the current state.
                // if (self.iter == null) {
                if self.dir.is_none() {
                    match self.state {
                        // .system => {
                        State::System => {
                            // const path = self.systemFontsPath() orelse {
                            //     self.state = .user; continue;
                            // };
                            let Some(path) = system_fonts_path() else {
                                self.state = State::User;
                                continue;
                            };
                            // self.system_path = path;
                            self.system_path = Some(path.clone());
                            // self.dir = std.Io.Dir.openDirAbsolute(io, path, .{ .iterate = true })
                            //     catch { self.state = .user; continue; };
                            // self.iter = self.dir.?.iterate();
                            match std::fs::read_dir(&path) {
                                Ok(d) => self.dir = Some(d),
                                Err(_) => {
                                    self.state = State::User;
                                    continue;
                                }
                            }
                        }
                        // .user => {
                        State::User => {
                            // const path = self.userFontsPath() orelse {
                            //     self.state = .done; continue;
                            // };
                            let Some(path) = user_fonts_path() else {
                                self.state = State::Done;
                                continue;
                            };
                            // self.user_path = path;
                            self.user_path = Some(path.clone());
                            match std::fs::read_dir(&path) {
                                Ok(d) => self.dir = Some(d),
                                Err(_) => {
                                    self.state = State::Done;
                                    continue;
                                }
                            }
                        }
                        // .done => return null,
                        State::Done => return Ok(None),
                    }
                }

                // const entry = (self.iter.?.next() catch null) orelse { ... };
                //
                // PORT NOTE: the reference's `catch null` folds a read error into
                // end-of-directory. `ReadDir::next` yields `Option<io::Result<_>>`,
                // so `None` (end) and `Some(Err)` (read error) fold the same way.
                let entry = match self.dir.as_mut().unwrap().next() {
                    Some(Ok(e)) => e,
                    // Finished this directory; advance state.
                    _ => {
                        // if (self.dir) |*d| d.close(io); self.dir = null; self.iter = null;
                        self.dir = None;
                        // self.state = switch (self.state) {
                        //     .system => .user, .user => .done, .done => .done };
                        self.state = match self.state {
                            State::System => State::User,
                            State::User => State::Done,
                            State::Done => State::Done,
                        };
                        continue;
                    }
                };

                // if (entry.kind != .file) continue;
                match entry.file_type() {
                    Ok(t) if t.is_file() => {}
                    _ => continue,
                }
                let name = entry.file_name();
                // if (!isFontFile(entry.name)) continue;
                if !is_font_file(&name) {
                    continue;
                }

                // if (try self.tryMatch(entry.name)) |face| return face;
                if let Some(face) = self.try_match(&name)? {
                    return Ok(Some(face));
                }
            }
        }

        // fn tryMatch(self: *DiscoverIterator, name: []const u8) !?DeferredFace {
        fn try_match(&mut self, name: &OsStr) -> Result<Option<DeferredFace>, Error> {
            // const dir_path = switch (self.state) {
            //     .system => self.system_path.?, .user => self.user_path.?,
            //     .done => return null,
            // };
            let dir_path = match self.state {
                State::System => self.system_path.as_ref().unwrap(),
                State::User => self.user_path.as_ref().unwrap(),
                State::Done => return Ok(None),
            };

            // var path_buf: [std.fs.max_path_bytes]u8 = undefined;
            // const full_path = std.fmt.bufPrintZ(&path_buf, "{s}\\{s}", .{ dir_path, name })
            //     catch return null;
            //
            // PORT NOTE: the reference formats into a fixed `max_path_bytes` stack
            // buffer and returns `null` on overflow. `CString` allocates, so the
            // only failure mode left is an interior NUL in the path — which is
            // exactly the same "unusable path, skip it" outcome, so it also
            // returns `null`.
            let mut full_path_bytes: Vec<u8> = dir_path.as_encoded_bytes().to_vec();
            full_path_bytes.push(b'\\');
            full_path_bytes.extend_from_slice(name.as_encoded_bytes());
            let Ok(full_path) = CString::new(full_path_bytes) else {
                return Ok(None);
            };

            // const is_ttc = std.ascii.endsWithIgnoreCase(name, ".ttc");
            let is_ttc = ends_with_ignore_case(name.as_encoded_bytes(), b".ttc");
            // const max_faces: i32 = if (is_ttc) 16 else 1;
            let max_faces: i32 = if is_ttc { 16 } else { 1 };

            // Probe each face in the file.
            // var face_index: i32 = 0;
            // while (face_index < max_faces) : (face_index += 1) {
            let mut face_index: i32 = 0;
            while face_index < max_faces {
                // var face = Face.initFile(self.lib, full_path, face_index,
                //     .{ .size = .{ .points = 12 } }) catch break;
                let opts = Options {
                    size: DesiredSize {
                        points: 12.0,
                        xdpi: DEFAULT_DPI,
                        ydpi: DEFAULT_DPI,
                    },
                    freetype_load_flags: Default::default(),
                };
                let Ok(mut face) = Face::init_file(self.lib, &full_path, face_index, opts) else {
                    break;
                };

                // if (self.matches(&face)) return try self.makeDeferred(face, full_path, face_index);
                if self.matches(&face) {
                    return self.make_deferred(face, full_path, face_index).map(Some);
                }

                // face.deinit();
                face.deinit();
                face_index += 1;
            }

            // return null;
            Ok(None)
        }

        /// Check whether the given face matches the descriptor.
        // fn matches(self: *const DiscoverIterator, face: *Face) bool {
        fn matches(&self, face: &Face) -> bool {
            // if (self.desc.family) |family| { if (!familyMatches(face, family)) return false; }
            if let Some(family) = self.desc.family.as_ref() {
                if !family_matches(face, family) {
                    return false;
                }
            }
            // if (self.desc.codepoint != 0) { if (face.glyphIndex(self.desc.codepoint) == null) return false; }
            if self.desc.codepoint != 0 && face.glyph_index(self.desc.codepoint).is_none() {
                return false;
            }
            // return true;
            true
        }

        // fn makeDeferred(self, face: Face, full_path: []const u8, face_index: i32) !DeferredFace {
        fn make_deferred(
            &self,
            face: Face,
            full_path: CString,
            face_index: i32,
        ) -> Result<DeferredFace, Error> {
            // const path_owned = try self.alloc.dupeZ(u8, full_path);
            // errdefer self.alloc.free(path_owned);
            //
            // (`full_path` is already an owning `CString` here — the reference's
            // `dupeZ` exists only because its `full_path` borrows a stack buffer.)
            let path_owned = full_path;

            // const presentation: Presentation = if (face.hasColor()) .emoji else .text;
            let presentation = if face.has_color() {
                Presentation::Emoji
            } else {
                Presentation::Text
            };

            // return DeferredFace{ .win = .{
            //     .path = path_owned, .face_index = face_index, .variations = self.variations,
            //     .peek = face, .presentation = presentation, .alloc = self.alloc,
            // } };
            Ok(DeferredFace {
                win: Some(WindowsData {
                    path: path_owned,
                    face_index,
                    variations: self.variations.clone(),
                    peek: face,
                    presentation,
                }),
            })
        }
    }

    /// Build the system fonts directory from %SYSTEMROOT%. Returns null
    /// if SYSTEMROOT is unset, which shouldn't happen on a healthy
    /// Windows install but we just skip the directory rather than
    /// falling back to a hardcoded drive letter.
    // fn systemFontsPath(self: *DiscoverIterator) ?[:0]const u8 {
    //     const systemroot = global.environ().getAlloc(self.alloc, "SYSTEMROOT") catch return null;
    //     defer self.alloc.free(systemroot);
    //     return std.fmt.allocPrintSentinel(self.alloc, "{s}\\Fonts", .{systemroot}, 0) catch null;
    // }
    //
    // PORT NOTE: a free function rather than a method — it reads nothing from
    // `self` but the (now-absent) allocator field.
    fn system_fonts_path() -> Option<OsString> {
        let systemroot = std::env::var_os("SYSTEMROOT")?;
        Some(join_ascii(&systemroot, b"\\Fonts"))
    }

    // fn userFontsPath(self: *DiscoverIterator) ?[:0]const u8 {
    //     const local_appdata = global.environ().getAlloc(self.alloc, "LOCALAPPDATA") catch return null;
    //     defer self.alloc.free(local_appdata);
    //     return std.fmt.allocPrintSentinel(self.alloc, "{s}\\Microsoft\\Windows\\Fonts",
    //         .{local_appdata}, 0) catch null;
    // }
    fn user_fonts_path() -> Option<OsString> {
        let local_appdata = std::env::var_os("LOCALAPPDATA")?;
        Some(join_ascii(&local_appdata, b"\\Microsoft\\Windows\\Fonts"))
    }

    /// Append an ASCII suffix to an `OsStr` without going through a lossy
    /// `String`. Sound: `OsStr::from_encoded_bytes_unchecked` accepts a valid
    /// encoding concatenated with ASCII (the documented safety contract).
    fn join_ascii(base: &OsStr, suffix: &[u8]) -> OsString {
        debug_assert!(suffix.is_ascii());
        let mut bytes = base.as_encoded_bytes().to_vec();
        bytes.extend_from_slice(suffix);
        // SAFETY: `bytes` is `base`'s own encoded form with an ASCII suffix
        // appended, which `OsStr`'s encoding explicitly permits.
        unsafe { OsStr::from_encoded_bytes_unchecked(&bytes) }.to_os_string()
    }

    /// `std.ascii.endsWithIgnoreCase`
    fn ends_with_ignore_case(haystack: &[u8], needle: &[u8]) -> bool {
        haystack.len() >= needle.len()
            && haystack[haystack.len() - needle.len()..].eq_ignore_ascii_case(needle)
    }

    /// `std.ascii.eqlIgnoreCase`
    fn eql_ignore_case(a: &[u8], b: &[u8]) -> bool {
        a.len() == b.len() && a.eq_ignore_ascii_case(b)
    }

    // fn isFontFile(name: []const u8) bool {
    //     return std.ascii.endsWithIgnoreCase(name, ".ttf") or
    //         std.ascii.endsWithIgnoreCase(name, ".ttc") or
    //         std.ascii.endsWithIgnoreCase(name, ".otf");
    // }
    fn is_font_file(name: &OsStr) -> bool {
        let b = name.as_encoded_bytes();
        ends_with_ignore_case(b, b".ttf")
            || ends_with_ignore_case(b, b".ttc")
            || ends_with_ignore_case(b, b".otf")
    }

    /// Compare a face's family against a requested family name. Checks
    /// FreeType's family_name first, then falls back to the SFNT name
    /// table entry.
    // fn familyMatches(face: *Face, family: [:0]const u8) bool {
    //     const ft_family: ?[*:0]const u8 = face.face.handle.*.family_name;
    //     if (ft_family) |f| { if (std.ascii.eqlIgnoreCase(std.mem.span(f), family)) return true; }
    //     var buf: [256]u8 = undefined;
    //     const sfnt = face.name(&buf) catch "";
    //     return sfnt.len > 0 and std.ascii.eqlIgnoreCase(sfnt, family);
    // }
    fn family_matches(face: &Face, family: &[u8]) -> bool {
        // const ft_family: ?[*:0]const u8 = face.face.handle.*.family_name;
        //
        // SAFETY: `face.face.handle` is the live `FT_Face` this `Face` owns;
        // `family_name` is either NULL or a NUL-terminated FreeType-owned string.
        let ft_family = unsafe { (*face.face.handle).family_name };
        if !ft_family.is_null() {
            let f = unsafe { std::ffi::CStr::from_ptr(ft_family) };
            if eql_ignore_case(f.to_bytes(), family) {
                return true;
            }
        }

        // var buf: [256]u8 = undefined;
        let mut buf = [0u8; 256];
        // const sfnt = face.name(&buf) catch "";
        let sfnt = face.name(&mut buf).unwrap_or(&[]);
        // return sfnt.len > 0 and std.ascii.eqlIgnoreCase(sfnt, family);
        !sfnt.is_empty() && eql_ignore_case(sfnt, family)
    }
}

/// The reference's `break :head opentype.Head.init(ptr[0..len]) catch |err| {
/// log.warn("error parsing head table: {}", .{err}); break :head null; }`
/// (`discovery.zig:756-764`), extracted into a free fn so the FAILURE arm is
/// testable without needing a CoreText font whose `head` table is malformed.
/// The production call site passes the copied table bytes verbatim.
#[cfg(target_os = "macos")]
fn parse_head_table(bytes: &[u8]) -> Option<rustty_font::opentype::head::Head> {
    match rustty_font::opentype::head::Head::init(bytes) {
        Ok(v) => Some(v),
        Err(err) => {
            log::warn!(target: LOG_SCOPE, "error parsing head table: {err:?}");
            None
        }
    }
}

/// The `OS/2` sibling of [`parse_head_table`] (`discovery.zig:773-781`).
#[cfg(target_os = "macos")]
fn parse_os2_table(bytes: &[u8]) -> Option<rustty_font::opentype::os2::OS2> {
    match rustty_font::opentype::os2::OS2::init(bytes) {
        Ok(v) => Some(v),
        Err(err) => {
            log::warn!(target: LOG_SCOPE, "error parsing OS/2 table: {err:?}");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // test "descriptor hash" {
    #[test]
    fn descriptor_hash() {
        // const testing = std.testing;
        // var d: Descriptor = .{};
        let d: Descriptor = Descriptor::default();
        // try testing.expect(d.hashcode() != 0);
        assert!(d.hashcode() != 0);
    }

    // test "descriptor hash family names" {
    #[test]
    fn descriptor_hash_family_names() {
        // const testing = std.testing;
        // var d1: Descriptor = .{ .family = "A" };
        let d1 = Descriptor {
            family: Some(b"A".to_vec()),
            ..Descriptor::default()
        };
        // var d2: Descriptor = .{ .family = "B" };
        let d2 = Descriptor {
            family: Some(b"B".to_vec()),
            ..Descriptor::default()
        };
        // try testing.expect(d1.hashcode() != d2.hashcode());
        assert!(d1.hashcode() != d2.hashcode());
    }

    /// Compile-gate: on every target that is neither macOS nor wasm32 (the
    /// `fontconfig_freetype` backend class), the `Discover` seam that
    /// `CodepointResolver`/`SharedGridSet` name unconditionally must type-check
    /// against the reference signatures (`discovery.zig`'s `Fontconfig`).
    ///
    /// PORT NOTE: this mirrors the `refAllDecls` convention used across this crate
    /// (e.g. `face/mod.rs`'s `ref_all_decls`): the methods are coerced to function
    /// pointers with their exact expected signatures rather than CALLED, so the
    /// gate holds without needing a live system font database in the test
    /// environment. Coercion still fails to compile if a signature drifts, which
    /// is exactly the guarantee this test exists to hold. (The bodies themselves
    /// are the live Fontconfig discoverer — they are exercised end-to-end by the
    /// ungated `SharedGridSet` / discovered-font shaping tests on Linux.)
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_arch = "wasm32")))]
    #[test]
    fn discover_impl_typechecks() {
        use crate::collection::Collection;
        use crate::deferred_face::DeferredFace;
        use crate::library::Library;

        // The active discoverer on this backend class is the Fontconfig one.
        let _: fn(Library) -> Fontconfig = Fontconfig::init;
        let _: fn(&mut Fontconfig) = Fontconfig::deinit;
        let _: fn(&Fontconfig, Descriptor) -> Result<DiscoverIterator, Error> =
            Fontconfig::discover;
        let _: fn(&Fontconfig, *mut Collection, Descriptor) -> Result<DiscoverIterator, Error> =
            Fontconfig::discover_fallback;

        // `SharedGridSet`/`CodepointResolver` drive the iterator via `next`/`deinit`.
        let _: fn(&mut DiscoverIterator) -> Result<Option<DeferredFace>, Error> =
            DiscoverIterator::next;
        let _: fn(&mut DiscoverIterator) = DiscoverIterator::deinit;

        // `Discover` (the backend selector) must resolve to that discoverer.
        let _: fn(Library) -> Discover = Discover::init;

        // `SharedGridSet::Error` composes the discovery error via `From`.
        let _: fn(crate::face::FaceError) -> Error = Error::from;
        let _: fn(crate::collection::Error) -> Error = Error::from;
    }

    /// Task R13-P (`NEW-FINDINGS.md` N51): a `font-family` / `font-style`
    /// config value can hold a NUL byte (the config line reader keeps it), and
    /// `to_fc_pattern` used to `.expect` on it and panic. The reference adds the
    /// `[:0]` pointer to the pattern, so fontconfig sees the bytes before the
    /// NUL; the port now stores exactly those bytes, which this reads back out
    /// of the pattern.
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_arch = "wasm32")))]
    #[test]
    fn a_descriptor_family_with_a_nul_reaches_the_pattern_cut_at_the_nul() {
        let _fc = crate::fc::global_shared();

        let desc = Descriptor {
            family: Some(b"Fir\x00a Code".to_vec()),
            style: Some(b"Bo\x00ld".to_vec()),
            codepoint: 0,
            size: 0.0,
            bold: false,
            italic: false,
            monospace: false,
            variations: Vec::new(),
        };

        let pat_ptr = desc.to_fc_pattern();
        // SAFETY: `to_fc_pattern` hands back a freshly created pattern this test
        // owns and destroys below.
        let pat = unsafe { &mut *pat_ptr };

        assert!(
            matches!(
                pat.get(crate::fc::Property::Family, 0),
                Ok(crate::fc::Value::String(s)) if s.to_bytes() == b"Fir"
            ),
            "the family fontconfig sees is the bytes before the NUL"
        );
        assert!(
            matches!(
                pat.get(crate::fc::Property::Style, 0),
                Ok(crate::fc::Value::String(s)) if s.to_bytes() == b"Bo"
            ),
            "the style fontconfig sees is the bytes before the NUL"
        );

        pat.destroy();
    }

    // test "fontconfig" {
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_arch = "wasm32")))]
    #[test]
    fn fontconfig() {
        // Shared: fontconfig's global state must not be torn down under us
        // (see `fc::FC_GLOBAL`).
        let _fc = crate::fc::global_shared();
        // if (options.backend != .fontconfig_freetype) return error.SkipZigTest;
        //
        // PORT NOTE: the reference's `!= fontconfig_freetype` skip is the
        // `#[cfg]` on this test: `options::BACKEND` is `FontconfigFreetype` on
        // exactly that target set (`Backend::default()`), so a runtime guard here
        // could never fire (R14: no silent early return). On the freetype-active
        // backend class the body discovers a real system font.

        use crate::library::Library;

        // var lib = try Library.init(alloc); defer lib.deinit();
        let mut lib = Library::init().unwrap();

        // var fc = Fontconfig.init(lib); defer fc.deinit();
        let mut fc = Fontconfig::init(lib);

        // var it = try fc.discover(alloc, .{ .family = "monospace", .size = 12 });
        // defer it.deinit();
        let mut it = fc
            .discover(Descriptor {
                family: Some(b"monospace".to_vec()),
                size: 12.0,
                ..Descriptor::default()
            })
            .unwrap();

        // defers run in reverse: it.deinit, fc.deinit, lib.deinit.
        it.deinit();
        fc.deinit();
        lib.deinit();
    }

    /// W9F-01 proving test (no reference counterpart — the reference gets this
    /// for free from `defer disco_it.deinit()` at every construction site).
    ///
    /// Rustty's `DiscoverIterator` owns raw `FcFontSet`/`FcPattern` handles, and
    /// the call sites that only COMMENT the reference's defer (shared_grid_set,
    /// codepoint_resolver) relied on a "RAII scope drop" that did not exist.
    /// Asserts (a) dropping an iterator destroys its handles exactly once, and
    /// (b) `deinit` is idempotent, so the explicit `deinit()` calls the port
    /// already makes do not double-destroy when the drop follows.
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_arch = "wasm32")))]
    #[test]
    fn discover_iterator_drops_native_handles() {
        // Shared: fontconfig's global state must not be torn down under us
        // (see `fc::FC_GLOBAL`).
        let _fc = crate::fc::global_shared();
        // The backend skip is the `#[cfg]` above (see `fontconfig`, R14).

        use super::fontconfig_impl::deinit_destroys;
        use crate::library::Library;

        let mut lib = Library::init().unwrap();
        let mut fc = Fontconfig::init(lib);

        let desc = Descriptor {
            family: Some(b"monospace".to_vec()),
            size: 12.0,
            ..Descriptor::default()
        };

        let before = deinit_destroys();

        // (a) An iterator that is never explicitly deinited — the shape of every
        // early exit at the call sites — must still free its handles on drop.
        {
            let _it = fc.discover(desc.clone()).unwrap();
        }
        assert_eq!(
            deinit_destroys() - before,
            1,
            "dropping a DiscoverIterator must destroy its FcFontSet/FcPattern \
             (the reference's `defer disco_it.deinit()`)"
        );

        // (b) An explicit `deinit()` followed by the drop must destroy ONCE.
        let mut it = fc.discover(desc).unwrap();
        it.deinit();
        drop(it);
        assert_eq!(
            deinit_destroys() - before,
            2,
            "deinit() must be idempotent: the drop after an explicit deinit must \
             not destroy the handles a second time"
        );

        fc.deinit();
        lib.deinit();
    }

    // test "fontconfig codepoint" {
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_arch = "wasm32")))]
    #[test]
    fn fontconfig_codepoint() {
        // Shared: fontconfig's global state must not be torn down under us
        // (see `fc::FC_GLOBAL`).
        let _fc = crate::fc::global_shared();
        // if (options.backend != .fontconfig_freetype) return error.SkipZigTest;
        // (the `#[cfg]` above IS that skip, R14 — see `fontconfig`)

        use crate::library::Library;

        // var lib = try Library.init(alloc); defer lib.deinit();
        let mut lib = Library::init().unwrap();

        // var fc = Fontconfig.init(lib); defer fc.deinit();
        let mut fc = Fontconfig::init(lib);

        // var it = try fc.discover(alloc, .{ .codepoint = 'A', .size = 12 });
        // defer it.deinit();
        let mut it = fc
            .discover(Descriptor {
                codepoint: u32::from('A'),
                size: 12.0,
                ..Descriptor::default()
            })
            .unwrap();

        // The first result should have the codepoint. Later ones may not
        // because fontconfig returns all fonts sorted.
        // var face = (try it.next()).?; defer face.deinit();
        let mut face = it.next().unwrap().unwrap();

        // try testing.expect(face.hasCodepoint('A', null));
        assert!(face.has_codepoint(u32::from('A'), None));

        // Should have other codepoints too
        // try testing.expect(face.hasCodepoint('B', null));
        assert!(face.has_codepoint(u32::from('B'), None));

        // defers run in reverse: face.deinit, it.deinit, fc.deinit, lib.deinit.
        face.deinit();
        it.deinit();
        fc.deinit();
        lib.deinit();
    }

    // PORT NOTE: unit tests for `Descriptor::to_fc_pattern` (the port of
    // `discovery.zig:117-157` `toFcPattern`). The reference has no dedicated test
    // for `toFcPattern` (it is exercised indirectly by the `fontconfig`/`fontconfig
    // codepoint` tests, whose bodies discover real fonts); these directly assert
    // the built pattern's contents, and are gated to the fontconfig backend class
    // (the only class where `to_fc_pattern` is compiled).
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_arch = "wasm32")))]
    #[test]
    fn descriptor_to_fc_pattern_family_size_mono() {
        // Shared: fontconfig's global state must not be torn down under us
        // (see `fc::FC_GLOBAL`).
        let _fc = crate::fc::global_shared();
        use crate::fc::{Property, Spacing, Value};

        let desc = Descriptor {
            family: Some(b"monospace".to_vec()),
            size: 12.0,
            ..Descriptor::default()
        };
        // SAFETY: `to_fc_pattern` returns a freshly created, non-null pattern that
        // this test owns and destroys below.
        let pat = unsafe { &mut *desc.to_fc_pattern() };

        let Value::String(family) = pat.get(Property::Family, 0).unwrap() else {
            panic!("family value was not a string");
        };
        assert_eq!(family, c"monospace");

        let Value::Integer(size) = pat.get(Property::Size, 0).unwrap() else {
            panic!("size value was not an integer");
        };
        assert_eq!(size, 12);

        // Spacing is added UNCONDITIONALLY (the reference always biases toward
        // monospace matches, `discovery.zig:147-154`).
        let Value::Integer(spacing) = pat.get(Property::Spacing, 0).unwrap() else {
            panic!("spacing value was not an integer");
        };
        assert_eq!(spacing, Spacing::Mono as i32);

        pat.destroy();
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows", target_arch = "wasm32")))]
    #[test]
    fn descriptor_to_fc_pattern_codepoint() {
        // Shared: fontconfig's global state must not be torn down under us
        // (see `fc::FC_GLOBAL`).
        let _fc = crate::fc::global_shared();
        use crate::fc::{Property, Value};

        let desc = Descriptor {
            codepoint: u32::from('A'),
            ..Descriptor::default()
        };
        // SAFETY: as above — owned, non-null, destroyed below.
        let pat = unsafe { &mut *desc.to_fc_pattern() };

        let Value::CharSet(cs) = pat.get(Property::Charset, 0).unwrap() else {
            panic!("charset value was not a char set");
        };
        // SAFETY: `cs` borrows the pattern-owned char set; valid until `pat` is
        // destroyed.
        assert!(unsafe { (*cs).has_char(u32::from('A')) });

        pat.destroy();
    }

    /// JetBrains Mono with its `name` table replaced by one whose family,
    /// full and typographic-family names are `family` (Windows, UTF-16BE) and
    /// whose PostScript name is `ps`. The new table is appended and the table
    /// directory entry repointed; CoreText does not check the checksums.
    #[cfg(target_os = "macos")]
    fn font_named(family: &str, ps: &str) -> Vec<u8> {
        let utf16be =
            |s: &str| -> Vec<u8> { s.encode_utf16().flat_map(|u| u.to_be_bytes()).collect() };
        let records: [(u16, Vec<u8>); 5] = [
            (1, utf16be(family)),
            (2, utf16be("Regular")),
            (4, utf16be(family)),
            (6, utf16be(ps)),
            (16, utf16be(family)),
        ];
        let mut table = Vec::new();
        let count = records.len() as u16;
        let string_offset = 6 + 12 * count;
        table.extend_from_slice(&0u16.to_be_bytes()); // format 0
        table.extend_from_slice(&count.to_be_bytes());
        table.extend_from_slice(&string_offset.to_be_bytes());
        let mut storage = Vec::new();
        for (name_id, bytes) in &records {
            // platform 3 (Windows), encoding 1 (Unicode BMP), language 0x409
            for v in [
                3u16,
                1,
                0x409,
                *name_id,
                bytes.len() as u16,
                storage.len() as u16,
            ] {
                table.extend_from_slice(&v.to_be_bytes());
            }
            storage.extend_from_slice(bytes);
        }
        table.extend_from_slice(&storage);

        let mut font = rustty_font::embedded::VARIABLE.to_vec();
        while !font.len().is_multiple_of(4) {
            font.push(0);
        }
        let offset = font.len() as u32;
        let num_tables = u16::from_be_bytes([font[4], font[5]]) as usize;
        let rec = (0..num_tables)
            .map(|i| 12 + 16 * i)
            .find(|&r| &font[r..r + 4] == b"name")
            .expect("JetBrains Mono has a name table");
        font[rec + 8..rec + 12].copy_from_slice(&offset.to_be_bytes());
        font[rec + 12..rec + 16].copy_from_slice(&(table.len() as u32).to_be_bytes());
        font.extend_from_slice(&table);
        font
    }

    /// A font registered with CoreText for this process until dropped
    /// (`CTFontManagerRegisterGraphicsFont`).
    #[cfg(target_os = "macos")]
    struct RegisteredFont(*mut core::ffi::c_void);

    #[cfg(target_os = "macos")]
    extern "C" {
        fn CGDataProviderCreateWithCFData(data: *const core::ffi::c_void)
            -> *mut core::ffi::c_void;
        fn CGDataProviderRelease(p: *mut core::ffi::c_void);
        fn CGFontCreateWithDataProvider(p: *mut core::ffi::c_void) -> *mut core::ffi::c_void;
        fn CGFontRelease(f: *mut core::ffi::c_void);
        fn CTFontManagerRegisterGraphicsFont(
            font: *mut core::ffi::c_void,
            error: *mut *mut core::ffi::c_void,
        ) -> bool;
        fn CTFontManagerUnregisterGraphicsFont(
            font: *mut core::ffi::c_void,
            error: *mut *mut core::ffi::c_void,
        ) -> bool;
    }

    #[cfg(target_os = "macos")]
    impl RegisteredFont {
        fn new(bytes: &[u8]) -> RegisteredFont {
            let data = objc2_core_foundation::CFData::from_bytes(bytes);
            // SAFETY: plain CoreGraphics/CoreText calls on live objects; each
            // create is balanced by a release below or in `drop`.
            unsafe {
                let provider = CGDataProviderCreateWithCFData(
                    (&*data as *const objc2_core_foundation::CFData).cast(),
                );
                assert!(!provider.is_null());
                let font = CGFontCreateWithDataProvider(provider);
                CGDataProviderRelease(provider);
                assert!(!font.is_null(), "CoreGraphics accepts the font");
                assert!(
                    CTFontManagerRegisterGraphicsFont(font, core::ptr::null_mut()),
                    "CoreText registers the font"
                );
                RegisteredFont(font)
            }
        }
    }

    #[cfg(target_os = "macos")]
    impl Drop for RegisteredFont {
        fn drop(&mut self) {
            // SAFETY: `self.0` is the registered CGFont this value owns.
            unsafe {
                CTFontManagerUnregisterGraphicsFont(self.0, core::ptr::null_mut());
                CGFontRelease(self.0);
            }
        }
    }

    /// S87-F2: `discoverExactFamily` reads the found font's family name into a
    /// 256-byte buffer (`discovery.zig:429-441`); a name that does not fit with
    /// its NUL (256 bytes or more) is not found, even when it is the family
    /// asked for. The fonts are registered for this process under names no
    /// other font has; the 255-byte one proves the lookup itself finds them.
    #[cfg(target_os = "macos")]
    #[test]
    fn discover_exact_family_rejects_names_that_do_not_fit_256_bytes() {
        let disco = CoreText::init(crate::library::Library::init().unwrap());
        let long = |n: usize, tag: char| -> String {
            let mut s = format!("Rustty S87F2 {tag} ");
            while s.len() < n {
                s.push('x');
            }
            s
        };

        let fits = long(255, 'a');
        let too_long = long(256, 'b');
        assert_eq!((fits.len(), too_long.len()), (255, 256));
        let _fits_font = RegisteredFont::new(&font_named(&fits, "RusttyS87F2a"));
        let _too_long_font = RegisteredFont::new(&font_named(&too_long, "RusttyS87F2b"));

        // Precondition: CoreText finds each font by that name and reports it
        // as the family (so only the buffer can make the difference).
        for name in [&fits, &too_long] {
            let s = crate::deferred_face::new_cfstring(name).unwrap();
            let f = unsafe { objc2_core_text::CTFont::with_name(&s, 12.0, core::ptr::null()) };
            assert_eq!(unsafe { f.family_name() }.to_string(), *name);
        }

        let found = disco.discover_exact_family(fits.as_bytes()).unwrap();
        assert!(found.is_some(), "a 255-byte family fits the buffer");
        let not_found = disco.discover_exact_family(too_long.as_bytes()).unwrap();
        assert!(not_found.is_none(), "a 256-byte family does not");
    }

    // test "coretext" {
    #[cfg(target_os = "macos")]
    #[test]
    fn coretext() {
        // if (options.backend != .coretext and options.backend != .coretext_freetype)
        //     return error.SkipZigTest;
        // (the `#[cfg(target_os = "macos")]` above IS that skip: `options::BACKEND`
        // is `Coretext` on every macOS build, so a runtime guard could never fire;
        // R14 — no silent early return)

        use crate::library::Library;

        // var lib = try Library.init(alloc); defer lib.deinit();
        let mut lib = Library::init().unwrap();

        // var ct = CoreText.init(lib); defer ct.deinit();
        let mut ct = CoreText::init(lib);
        // var it = try ct.discover(alloc, .{ .family = "Monaco", .size = 12 });
        let mut it = ct
            .discover(Descriptor {
                family: Some(b"Monaco".to_vec()),
                size: 12.0,
                ..Descriptor::default()
            })
            .unwrap();
        // defer it.deinit();
        // var count: usize = 0;
        let mut count: usize = 0;
        // while (try it.next()) |_| { count += 1; }
        while it.next().unwrap().is_some() {
            count += 1;
        }
        // try testing.expect(count > 0);
        assert!(count > 0);

        it.deinit();
        ct.deinit();
        lib.deinit();
    }

    // test "from descriptors no match" (pkg/macos/text/font_collection.zig:117-139)
    //
    // PORT NOTE (S36-F5): the reference pins this in its OWN CoreText wrapper —
    // `createMatchingFontDescriptors` returns NULL when nothing matches and the
    // wrapper turns that into an EMPTY array, which `from descriptors no match`
    // asserts with `list.getCount() == 0`. rustty has no `pkg/macos`: the objc2
    // framework crates are the ratified substitution, so that divergence is
    // re-implemented HERE, in `copy_matching_descriptors`' `None => return
    // result` arm. The test therefore lives where the code does, and drives it
    // through the public `discover()` the wrapper's callers use.
    #[cfg(target_os = "macos")]
    #[test]
    fn from_descriptors_no_match() {
        // (the `#[cfg(target_os = "macos")]` above IS that skip: `options::BACKEND`
        // is `Coretext` on every macOS build, so a runtime guard could never fire;
        // R14 — no silent early return)

        use crate::library::Library;

        let mut lib = Library::init().unwrap();
        let mut ct = CoreText::init(lib);
        // const name = try foundation.String.createWithBytes("ThisShouldNeverExist", ...);
        let mut it = ct
            .discover(Descriptor {
                family: Some(b"ThisShouldNeverExist".to_vec()),
                size: 12.0,
                ..Descriptor::default()
            })
            .unwrap();

        // try testing.expect(list.getCount() == 0);
        let mut count: usize = 0;
        while it.next().unwrap().is_some() {
            count += 1;
        }
        assert_eq!(0, count);

        it.deinit();
        ct.deinit();
        lib.deinit();
    }

    // test "coretext codepoint" {
    #[cfg(target_os = "macos")]
    #[test]
    fn coretext_codepoint() {
        // if (options.backend != .coretext and options.backend != .coretext_freetype)
        //     return error.SkipZigTest;
        // (the `#[cfg(target_os = "macos")]` above IS that skip: `options::BACKEND`
        // is `Coretext` on every macOS build, so a runtime guard could never fire;
        // R14 — no silent early return)

        use crate::library::Library;

        // var lib = try Library.init(alloc); defer lib.deinit();
        let mut lib = Library::init().unwrap();

        // var ct = CoreText.init(lib); defer ct.deinit();
        let mut ct = CoreText::init(lib);
        // var it = try ct.discover(alloc, .{ .codepoint = 'A', .size = 12 });
        let mut it = ct
            .discover(Descriptor {
                codepoint: u32::from('A'),
                size: 12.0,
                ..Descriptor::default()
            })
            .unwrap();
        // defer it.deinit();

        // The first result should have the codepoint. Later ones may not
        // because fontconfig returns all fonts sorted.
        // const face = (try it.next()).?;
        let face = it.next().unwrap().unwrap();
        // try testing.expect(face.hasCodepoint('A', null));
        assert!(face.has_codepoint(u32::from('A'), None));

        // Should have other codepoints too
        // try testing.expect(face.hasCodepoint('B', null));
        assert!(face.has_codepoint(u32::from('B'), None));

        it.deinit();
        ct.deinit();
        lib.deinit();
    }

    // test "coretext sorting" {
    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "SkipZigTest: the reference disables this test unconditionally \
                (`if (true) return error.SkipZigTest;`, SF Pro is not in CI); \
                its body is ported as comments only (R14)"]
    fn coretext_sorting() {
        // if (options.backend != .coretext and options.backend != .coretext_freetype)
        //     return error.SkipZigTest;
        // (the `#[cfg(target_os = "macos")]` above IS that skip: `options::BACKEND`
        // is `Coretext` on every macOS build, so a runtime guard could never fire;
        // R14 — no silent early return)

        // !!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!//
        // FIXME: Disabled for now because SF Pro is not available in CI
        //        The solution likely involves directly testing that the
        //        `sortMatchingDescriptors` function sorts a bundled test
        //        font correctly, instead of relying on the system fonts.
        // if (true) return error.SkipZigTest;
        // !!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!//
        //
        // R14: that unconditional skip is the `#[ignore]` above. Run on request,
        // the test FAILS rather than reporting `ok` for a body that does not exist.
        panic!(
            "coretext_sorting is disabled upstream (`if (true) return error.SkipZigTest;`); \
             its body is not ported"
        );

        // The remainder is unreachable (the reference's `if (true) return` always
        // skips); it is ported verbatim as reference below and never executes:
        //
        // var lib = try Library.init(alloc); defer lib.deinit();
        // var ct = CoreText.init(lib); defer ct.deinit();
        //
        // We try to get a Regular, Italic, Bold, & Bold Italic version of SF Pro,
        // which should be installed on all Macs, and has many styles which makes
        // it a good test, since there will be many results for each discovery.
        //
        // Regular
        // {
        //     var it = try ct.discover(alloc, .{ .family = "SF Pro", .size = 12 });
        //     defer it.deinit();
        //     const res = (try it.next()).?;
        //     var buf: [1024]u8 = undefined;
        //     const name = try res.name(&buf);
        //     try testing.expectEqualStrings("SF Pro Regular", name);
        // }
        //
        // Regular Italic
        // NOTE: This makes sure that we don't accidentally prefer "Thin Italic",
        //       which we previously did, because it has a shorter name.
        // {
        //     var it = try ct.discover(alloc, .{ .family = "SF Pro", .size = 12, .italic = true });
        //     defer it.deinit();
        //     const res = (try it.next()).?;
        //     var buf: [1024]u8 = undefined;
        //     const name = try res.name(&buf);
        //     try testing.expectEqualStrings("SF Pro Regular Italic", name);
        // }
        //
        // Bold
        // {
        //     var it = try ct.discover(alloc, .{ .family = "SF Pro", .size = 12, .bold = true });
        //     defer it.deinit();
        //     const res = (try it.next()).?;
        //     var buf: [1024]u8 = undefined;
        //     const name = try res.name(&buf);
        //     try testing.expectEqualStrings("SF Pro Bold", name);
        // }
        //
        // Bold Italic
        // {
        //     var it = try ct.discover(alloc, .{ .family = "SF Pro", .size = 12, .bold = true, .italic = true });
        //     defer it.deinit();
        //     const res = (try it.next()).?;
        //     var buf: [1024]u8 = undefined;
        //     const name = try res.name(&buf);
        //     try testing.expectEqualStrings("SF Pro Bold Italic", name);
        // }
    }

    // test "windows" {
    //
    // SKIP-STUB (the crate's established `fontconfig`/`web_canvas` precedent, and
    // VERIFICATION_DEBT V11): the body is LIVE as of WS20 and compiles only on
    // `target_os = "windows"`, where `Windows`/`DeferredFace.win` exist. On every
    // other host the `cfg_attr(.., ignore)` below reports it `ignored` — the
    // reference's own `if (options.backend != .freetype_windows) return
    // error.SkipZigTest;`, made visible (R14; `options::BACKEND` is
    // `FreetypeWindows` on exactly that target). It still needs a stock Windows
    // install to pass: it asserts `Arial` is discoverable and has a glyph for 'A'.
    #[test]
    #[cfg_attr(
        not(all(target_os = "windows", not(target_arch = "wasm32"))),
        ignore = "SkipZigTest: freetype_windows backend only (R14)"
    )]
    fn windows() {
        #[cfg(all(target_os = "windows", not(target_arch = "wasm32")))]
        {
            // if (options.backend != .freetype_windows) return error.SkipZigTest;

            use crate::library::Library;

            // var lib = try Library.init(alloc); defer lib.deinit();
            let mut lib = Library::init().unwrap();

            // var win = Windows.init(lib); defer win.deinit();
            let mut win = Windows::init(lib);

            // Arial ships on every stock Windows install.
            // var it = try win.discover(alloc, .{ .family = "Arial", .size = 12 });
            // defer it.deinit();
            let mut it = win
                .discover(Descriptor {
                    family: Some(b"Arial".to_vec()),
                    size: 12.0,
                    ..Descriptor::default()
                })
                .unwrap();

            // var face = (try it.next()) orelse return error.TestFontNotFound;
            // defer face.deinit();
            let mut face = it.next().unwrap().expect("TestFontNotFound");

            // try testing.expect(face.hasCodepoint('A', null));
            assert!(face.has_codepoint(u32::from('A'), None));

            // defers run in reverse: face.deinit, it.deinit, win.deinit, lib.deinit.
            face.deinit();
            it.deinit();
            win.deinit();
            lib.deinit();
        }
    }

    // -----------------------------------------------------------------------
    // task-14c-4 fix round 1 (F4): the two live reference `log.warn` sites in
    // `discovery.zig` (`:762`, `:779`) that NO ledger row named. Recorded as
    // finding N6.
    //
    // The production call sites feed these helpers bytes copied out of a
    // CoreText font table, and manufacturing a CTFont with a MALFORMED `head`
    // or `OS/2` table is not something this suite can do — which is why the arms
    // were extracted into free fns. The helpers take `&[u8]`, so the failure arm
    // is driven directly with bytes the parser must reject.
    // -----------------------------------------------------------------------

    /// Positive control: a malformed `head` table emits exactly ONE warn with
    /// the reference's message, at the reference's `.discovery` scope, and the
    /// parse still degrades to `None` (the reference's `break :head null`).
    #[cfg(target_os = "macos")]
    #[test]
    fn parse_head_table_warns_once_on_a_malformed_table() {
        let cap = crate::log_capture::start();
        // Far too short to be a `head` table (54 bytes).
        let parsed = super::parse_head_table(&[0u8; 3]);
        let messages = cap.messages();
        drop(cap);

        assert!(parsed.is_none());
        assert_eq!(1, messages.len(), "captured: {messages:?}");
        assert_eq!(log::Level::Warn, messages[0].0);
        assert_eq!("discovery", messages[0].1);
        assert!(
            messages[0].2.starts_with("error parsing head table: "),
            "unexpected message: {:?}",
            messages[0].2
        );
    }

    /// Negative control: a WELL-FORMED `head` table must emit nothing. Without
    /// this, the test above would still pass against a port that warned
    /// unconditionally — the success-path bug this stage is guarding against.
    #[cfg(target_os = "macos")]
    #[test]
    fn parse_head_table_is_silent_on_a_well_formed_table() {
        // A `head` table is 54 bytes of fixed-width big-endian fields with no
        // magic the parser validates, so a correctly SIZED buffer is a
        // well-formed one as far as `Head::init` is concerned. That is exactly
        // the discrimination under test: length, not content.
        let cap = crate::log_capture::start();
        let parsed = super::parse_head_table(&[0u8; 54]);
        let messages = cap.messages();
        drop(cap);

        assert!(parsed.is_some());
        assert_eq!(Vec::<crate::log_capture::CapturedRecord>::new(), messages);
    }

    /// The `OS/2` sibling of the positive control above.
    #[cfg(target_os = "macos")]
    #[test]
    fn parse_os2_table_warns_once_on_a_malformed_table() {
        let cap = crate::log_capture::start();
        let parsed = super::parse_os2_table(&[0u8; 3]);
        let messages = cap.messages();
        drop(cap);

        assert!(parsed.is_none());
        assert_eq!(1, messages.len(), "captured: {messages:?}");
        assert_eq!(log::Level::Warn, messages[0].0);
        assert_eq!("discovery", messages[0].1);
        assert!(
            messages[0].2.starts_with("error parsing OS/2 table: "),
            "unexpected message: {:?}",
            messages[0].2
        );
    }

    /// The `OS/2` sibling of the negative control above.
    #[cfg(target_os = "macos")]
    #[test]
    fn parse_os2_table_is_silent_on_a_well_formed_table() {
        let cap = crate::log_capture::start();
        let parsed = super::parse_os2_table(&[0u8; 128]);
        let messages = cap.messages();
        drop(cap);

        assert!(parsed.is_some());
        assert_eq!(Vec::<crate::log_capture::CapturedRecord>::new(), messages);
    }
}
