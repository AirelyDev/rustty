//! Thin, faithful safe wrappers around `fontconfig-sys` — the Rust port of the
//! reference's `pkg/fontconfig/*.zig` binding package (a hand-written safe layer
//! over the Fontconfig C API), sibling of [`crate::ft`] and [`crate::hb`].
//!
//! Unlike `ft.rs`/`hb.rs` (which are additive glue standing in for not-yet-carved
//! packages), `pkg/fontconfig` IS a carved reference target with its own test
//! contract: all 11 of its `test` blocks are ported verbatim into this module's
//! `#[cfg(test)] mod tests` (R1). The module mirrors the reference's own exports
//! (`main.zig`): the opaque handle types `Config`/`Pattern`/`CharSet`/`LangSet`/
//! `FontSet`, the `Weight`/`Slant`/`Spacing`/`Property`/`Result`/`MatchKind`
//! enums, the `Type`/`Value` value model, the `Error` set, and the `init.zig`
//! free functions.
//!
//! RAW-POINTER DISCIPLINE (matching the reference's `opaque {}` handles): the
//! five handle types are zero-field, `#[repr(transparent)]` reinterpretations of
//! the corresponding `fontconfig_sys` opaque C struct. They are manually managed
//! via explicit `destroy` — NOT borrow-checked lifetimes. Constructors return a
//! raw `*mut Handle` (exactly the reference's `*Handle`); callers dereference to
//! invoke methods and call `destroy` when done, just as the reference does.
//!
//! ERROR-ATOM NOTE (R1, the plan's two-error-atoms gotcha): [`Error`] is the
//! port of `pkg/fontconfig/error.zig`'s `Error` set, whose failure atom is
//! `FontconfigFailed` (lowercase "config"). This is DISTINCT from the ad hoc
//! `error.FontConfigFailed` (capital "C") raised in `src/font/discovery.zig:278`
//! — that atom belongs to the discovery layer (a later task), not this package,
//! and is deliberately NOT represented here.

use fontconfig_sys as c;
use std::ffi::CStr;

/// The `FcBool` truth value (`c.FcTrue`). Fontconfig defines `FcTrue` as `1`;
/// `bindgen` does not emit the `#define`, so it is reproduced here.
const FC_TRUE: c::FcBool = 1;
/// The `FcBool` false value (`c.FcFalse`), `0`. See [`FC_TRUE`].
const FC_FALSE: c::FcBool = 0;

// =============================================================================
// error.zig — the package error set.
// =============================================================================

/// The Fontconfig binding error set.
// PORT NOTE: `pkg/fontconfig/error.zig`'s `Error`. `FontconfigFailed` (lowercase
// "config") is this package's own failure atom — see the module ERROR-ATOM NOTE.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    OutOfMemory,
    FontconfigFailed,
    FontconfigNoMatch,
    FontconfigTypeMismatch,
    FontconfigNoId,
}

// =============================================================================
// init.zig — library initialization + version.
// =============================================================================

/// Initialize the default configuration (loads config + scans fonts), returning
/// `true` on success.
// PORT NOTE: `init.zig:5-7` `init`.
pub fn init() -> bool {
    unsafe { c::FcInit() == FC_TRUE }
}

/// Finalize the library, freeing the default configuration and all data.
// PORT NOTE: `init.zig:9-11` `fini`.
pub fn fini() {
    unsafe { c::FcFini() }
}

// PORT NOTE (parallel-test correctness — DELIBERATE DIVERGENCE from the Zig
// reference, the `rustty-oniguruma` `ensure_init` precedent): the reference's
// `test "init"` (`init.zig:30-33`) calls `FcInit()` and then `FcFini()`.
// `FcFini` tears down ALL of fontconfig's PROCESS-GLOBAL state — the default
// config, the directory-cache list and its mutex, and the interned object-type
// registry every `FcPattern` uses. Under Zig's `zig test`, which runs a file's
// tests SERIALLY, that teardown can never overlap another fontconfig user.
// Rust's `cargo test` runs `#[test]`s CONCURRENTLY, so that `FcFini` ran while
// sibling tests were inside `FcInitLoadConfigAndFonts` / `FcFontSort` /
// `FcDirCacheRead`. Observed on Linux/fontconfig 2.17: a deadlock parked in
// fontconfig's own `FcDirCacheMapFd` cache mutex, plus SIGSEGV and SIGABRT —
// 8 failures in 40 runs of the release test binary.
//
// This restores the reference's serialization exactly where it matters and
// nowhere else: the teardown test takes the lock EXCLUSIVELY, every other
// fontconfig-touching test takes it SHARED, so the suite still runs in
// parallel. Test-only — no production code path takes it.
#[cfg(test)]
static FC_GLOBAL: std::sync::RwLock<()> = std::sync::RwLock::new(());

/// Shared access to fontconfig's process-global state — held for the body of
/// every test that calls into fontconfig. See [`FC_GLOBAL`].
#[cfg(test)]
pub(crate) fn global_shared() -> std::sync::RwLockReadGuard<'static, ()> {
    // A panicking test poisons the lock; the state it guards is fontconfig's,
    // not ours, so recover rather than cascade a second failure.
    FC_GLOBAL.read().unwrap_or_else(|e| e.into_inner())
}

/// Exclusive access to fontconfig's process-global state — held only by the
/// `FcFini` teardown test. See [`FC_GLOBAL`].
#[cfg(test)]
pub(crate) fn global_exclusive() -> std::sync::RwLockWriteGuard<'static, ()> {
    FC_GLOBAL.write().unwrap_or_else(|e| e.into_inner())
}

/// Load the default configuration WITHOUT scanning fonts.
// PORT NOTE: `init.zig:13-15` `initLoadConfig`.
pub fn init_load_config() -> *mut Config {
    unsafe { c::FcInitLoadConfig().cast() }
}

/// Load the default configuration AND scan the system fonts.
// PORT NOTE: `init.zig:17-19` `initLoadConfigAndFonts`.
pub fn init_load_config_and_fonts() -> *mut Config {
    unsafe { c::FcInitLoadConfigAndFonts().cast() }
}

/// The Fontconfig library version, encoded as `major*10000 + minor*100 + micro`.
// PORT NOTE: `init.zig:21-23` `version`.
pub fn version() -> u32 {
    unsafe { c::FcGetVersion() as u32 }
}

// =============================================================================
// common.zig — the property/weight/slant/spacing/result/match enums.
// =============================================================================

/// Fontconfig weight constants.
// PORT NOTE: `common.zig:5-18` `Weight = enum(c_uint)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Weight {
    Thin = c::FC_WEIGHT_THIN,
    Extralight = c::FC_WEIGHT_EXTRALIGHT,
    Light = c::FC_WEIGHT_LIGHT,
    Demilight = c::FC_WEIGHT_DEMILIGHT,
    Book = c::FC_WEIGHT_BOOK,
    Regular = c::FC_WEIGHT_REGULAR,
    Medium = c::FC_WEIGHT_MEDIUM,
    Demibold = c::FC_WEIGHT_DEMIBOLD,
    Bold = c::FC_WEIGHT_BOLD,
    Extrabold = c::FC_WEIGHT_EXTRABOLD,
    Black = c::FC_WEIGHT_BLACK,
    Extrablack = c::FC_WEIGHT_EXTRABLACK,
}

/// Fontconfig slant constants.
// PORT NOTE: `common.zig:20-24` `Slant = enum(c_uint)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Slant {
    Roman = c::FC_SLANT_ROMAN,
    Italic = c::FC_SLANT_ITALIC,
    Oblique = c::FC_SLANT_OBLIQUE,
}

/// Fontconfig spacing constants.
// PORT NOTE: `common.zig:26-31` `Spacing = enum(c_uint)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Spacing {
    Proportional = c::FC_PROPORTIONAL,
    Dual = c::FC_DUAL,
    Mono = c::FC_MONO,
    Charcell = c::FC_CHARCELL,
}

/// Fontconfig pattern property (object) names.
// PORT NOTE: `common.zig:33-115` `Property`. The variant list mirrors the
// reference exactly; [`Property::cval`] reproduces the comptime
// underscore-stripped `FC_*` object-name table (`common.zig:85-108`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Property {
    Family,
    Style,
    Slant,
    Weight,
    Size,
    Aspect,
    PixelSize,
    Spacing,
    Foundry,
    Antialias,
    Hinting,
    HintStyle,
    VerticalLayout,
    Autohint,
    GlobalAdvance,
    Width,
    File,
    Index,
    FtFace,
    Rasterizer,
    Outline,
    Scalable,
    Color,
    Variable,
    Scale,
    Symbol,
    Dpi,
    Rgba,
    Minspace,
    Source,
    Charset,
    Lang,
    Fontversion,
    Fullname,
    Familylang,
    Stylelang,
    Fullnamelang,
    Capability,
    Embolden,
    EmbeddedBitmap,
    Decorative,
    LcdFilter,
    FontFeatures,
    FontVariations,
    Namelang,
    Prgname,
    Hash,
    PostscriptName,
    FontHasHint,
    Order,
}

impl Property {
    /// The NUL-terminated Fontconfig object name for this property: the enum
    /// variant name with underscores stripped (e.g. `pixel_size` → `pixelsize`).
    // PORT NOTE: `common.zig:85-108` `cval`. The reference builds this string in a
    // comptime context (a binary constant); Rust's C-string literals give the
    // same static, NUL-terminated constant with no allocation.
    pub fn cval(self) -> &'static CStr {
        match self {
            Property::Family => c"family",
            Property::Style => c"style",
            Property::Slant => c"slant",
            Property::Weight => c"weight",
            Property::Size => c"size",
            Property::Aspect => c"aspect",
            Property::PixelSize => c"pixelsize",
            Property::Spacing => c"spacing",
            Property::Foundry => c"foundry",
            Property::Antialias => c"antialias",
            Property::Hinting => c"hinting",
            Property::HintStyle => c"hintstyle",
            Property::VerticalLayout => c"verticallayout",
            Property::Autohint => c"autohint",
            Property::GlobalAdvance => c"globaladvance",
            Property::Width => c"width",
            Property::File => c"file",
            Property::Index => c"index",
            Property::FtFace => c"ftface",
            Property::Rasterizer => c"rasterizer",
            Property::Outline => c"outline",
            Property::Scalable => c"scalable",
            Property::Color => c"color",
            Property::Variable => c"variable",
            Property::Scale => c"scale",
            Property::Symbol => c"symbol",
            Property::Dpi => c"dpi",
            Property::Rgba => c"rgba",
            Property::Minspace => c"minspace",
            Property::Source => c"source",
            Property::Charset => c"charset",
            Property::Lang => c"lang",
            Property::Fontversion => c"fontversion",
            Property::Fullname => c"fullname",
            Property::Familylang => c"familylang",
            Property::Stylelang => c"stylelang",
            Property::Fullnamelang => c"fullnamelang",
            Property::Capability => c"capability",
            Property::Embolden => c"embolden",
            Property::EmbeddedBitmap => c"embeddedbitmap",
            Property::Decorative => c"decorative",
            Property::LcdFilter => c"lcdfilter",
            Property::FontFeatures => c"fontfeatures",
            Property::FontVariations => c"fontvariations",
            Property::Namelang => c"namelang",
            Property::Prgname => c"prgname",
            Property::Hash => c"hash",
            Property::PostscriptName => c"postscriptname",
            Property::FontHasHint => c"fonthashint",
            Property::Order => c"order",
        }
    }
}

/// The result of a Fontconfig lookup operation.
// PORT NOTE: `common.zig:117-133` `Result = enum(c_uint)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Result {
    Match,
    NoMatch,
    TypeMismatch,
    NoId,
    OutOfMemory,
}

impl Result {
    /// Map a raw `FcResult` to this enum.
    #[inline]
    fn from_c(v: c::FcResult) -> Result {
        match v {
            c::_FcResult_FcResultMatch => Result::Match,
            c::_FcResult_FcResultNoMatch => Result::NoMatch,
            c::_FcResult_FcResultTypeMismatch => Result::TypeMismatch,
            c::_FcResult_FcResultNoId => Result::NoId,
            c::_FcResult_FcResultOutOfMemory => Result::OutOfMemory,
            // Fontconfig only ever returns the five enumerators above.
            other => panic!("unexpected FcResult value {other}"),
        }
    }

    /// Convert a non-`Match` result into the corresponding [`Error`].
    // PORT NOTE: `common.zig:124-132` `toError`.
    pub fn to_error(self) -> std::result::Result<(), Error> {
        match self {
            Result::Match => Ok(()),
            Result::NoMatch => Err(Error::FontconfigNoMatch),
            Result::TypeMismatch => Err(Error::FontconfigTypeMismatch),
            Result::NoId => Err(Error::FontconfigNoId),
            Result::OutOfMemory => Err(Error::OutOfMemory),
        }
    }
}

/// The kind of matching to perform in `substituteWithPat`.
// PORT NOTE: `common.zig:135-139` `MatchKind = enum(c_uint)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatchKind {
    Pattern,
    Font,
    Scan,
}

impl MatchKind {
    #[inline]
    fn to_c(self) -> c::FcMatchKind {
        match self {
            MatchKind::Pattern => c::_FcMatchKind_FcMatchPattern,
            MatchKind::Font => c::_FcMatchKind_FcMatchFont,
            MatchKind::Scan => c::_FcMatchKind_FcMatchScan,
        }
    }
}

// =============================================================================
// value.zig — the tagged FcValue model.
// =============================================================================

/// The type tag of an [`Value`].
// PORT NOTE: `value.zig:9-21` `Type = enum(c_int)`. Carried in full for fidelity
// with the reference's exported surface.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum Type {
    Unknown = c::_FcType_FcTypeUnknown,
    Void = c::_FcType_FcTypeVoid,
    Integer = c::_FcType_FcTypeInteger,
    Double = c::_FcType_FcTypeDouble,
    String = c::_FcType_FcTypeString,
    Bool = c::_FcType_FcTypeBool,
    Matrix = c::_FcType_FcTypeMatrix,
    CharSet = c::_FcType_FcTypeCharSet,
    FtFace = c::_FcType_FcTypeFTFace,
    LangSet = c::_FcType_FcTypeLangSet,
    Range = c::_FcType_FcTypeRange,
}

/// A Fontconfig pattern value.
///
/// PORT NOTE (subset): `value.zig:23-70` models `Value` as a `union(Type)` over
/// all 11 `FcType`s. The discovery layer (the sole caller) only ever reads/writes
/// four of them — strings + integers via `toFcPattern`, and the `charset`/`lang`
/// handles via `pattern.get` (`src/font/discovery.zig:120-152, 331-332`) — so
/// only those four variants are carved here (per the task brief; extend as later
/// tasks require). The lifetime `'a` ties a borrowed [`Value::String`] to the
/// pattern it was read from (the reference's `[:0]const u8` slice into
/// pattern-owned memory).
#[derive(Clone, Copy, Debug)]
pub enum Value<'a> {
    Integer(i32),
    String(&'a CStr),
    CharSet(*const CharSet),
    LangSet(*const LangSet),
}

impl<'a> Value<'a> {
    /// Read a raw `FcValue` into a [`Value`].
    // PORT NOTE: `value.zig:36-50` `init`. Only the four carved types are handled;
    // any other type is a caller bug (this package is only ever handed
    // string/integer/charset/langset values — see the struct PORT NOTE) and
    // panics rather than silently mis-reading the union, mirroring the reference's
    // exhaustive (compile-checked) switch.
    fn init(cvalue: &c::FcValue) -> Value<'a> {
        match cvalue.type_ {
            c::_FcType_FcTypeInteger => Value::Integer(unsafe { cvalue.u.i }),
            c::_FcType_FcTypeString => Value::String(unsafe { CStr::from_ptr(cvalue.u.s.cast()) }),
            c::_FcType_FcTypeCharSet => Value::CharSet(unsafe { cvalue.u.c }.cast()),
            c::_FcType_FcTypeLangSet => Value::LangSet(unsafe { cvalue.u.l }.cast()),
            other => panic!("unmodeled FcValue type {other}"),
        }
    }

    /// Build a raw `FcValue` from this [`Value`].
    // PORT NOTE: `value.zig:52-69` `cval`.
    fn cval(self) -> c::FcValue {
        match self {
            Value::Integer(v) => c::FcValue {
                type_: c::_FcType_FcTypeInteger,
                u: c::_FcValue__bindgen_ty_1 { i: v },
            },
            Value::String(v) => c::FcValue {
                type_: c::_FcType_FcTypeString,
                u: c::_FcValue__bindgen_ty_1 {
                    s: v.as_ptr().cast(),
                },
            },
            Value::CharSet(v) => c::FcValue {
                type_: c::_FcType_FcTypeCharSet,
                u: c::_FcValue__bindgen_ty_1 { c: v.cast() },
            },
            Value::LangSet(v) => c::FcValue {
                type_: c::_FcType_FcTypeLangSet,
                u: c::_FcValue__bindgen_ty_1 { l: v.cast() },
            },
        }
    }
}

// =============================================================================
// config.zig — the Config handle.
// =============================================================================

/// An opaque Fontconfig configuration handle.
// PORT NOTE: `config.zig:11-56` `Config = opaque {}`. `#[repr(transparent)]` over
// the C `FcConfig` so `*mut Config` is layout-identical to `*mut c::FcConfig`
// (the reference's `@ptrCast` between `*Config` and `*c.struct__FcConfig`).
#[repr(transparent)]
pub struct Config(c::FcConfig);

/// The result of [`Config::font_sort`].
// PORT NOTE: `config.zig:58-61` `FontSortResult`.
pub struct FontSortResult {
    pub result: Result,
    pub fs: *mut FontSet,
}

impl Config {
    #[inline]
    fn cval(&mut self) -> *mut c::FcConfig {
        std::ptr::from_mut(self).cast()
    }

    /// Destroy this configuration.
    // PORT NOTE: `config.zig:12-14` `destroy`.
    pub fn destroy(&mut self) {
        unsafe { c::FcConfigDestroy(self.cval()) }
    }

    /// Return the fonts matching `pat`, sorted best-first. If `charset` is
    /// non-null it receives the union of coverage of the returned fonts.
    // PORT NOTE: `config.zig:20-36` `fontSort`.
    pub fn font_sort(
        &mut self,
        pat: &Pattern,
        trim: bool,
        charset: Option<*mut *mut CharSet>,
    ) -> FontSortResult {
        let mut result: c::FcResult = c::_FcResult_FcResultMatch;
        let fs = unsafe {
            c::FcFontSort(
                self.cval(),
                pat.cval(),
                if trim { FC_TRUE } else { FC_FALSE },
                charset.map_or(std::ptr::null_mut(), |p| p.cast()),
                &mut result,
            )
        };
        FontSortResult {
            result: Result::from_c(result),
            fs: fs.cast(),
        }
    }

    /// Create a pattern consisting of `pat` prepared for rendering with `font`.
    // PORT NOTE: `config.zig:38-43` `fontRenderPrepare`. Null → this package's
    // `Error.FontconfigFailed` (see the module ERROR-ATOM NOTE).
    pub fn font_render_prepare(
        &mut self,
        pat: &Pattern,
        font: &Pattern,
    ) -> std::result::Result<*mut Pattern, Error> {
        let p = unsafe { c::FcFontRenderPrepare(self.cval(), pat.cval(), font.cval()) };
        if p.is_null() {
            Err(Error::FontconfigFailed)
        } else {
            Ok(p.cast())
        }
    }

    /// Apply `kind` edits from this configuration to `pat`.
    // PORT NOTE: `config.zig:45-51` `substituteWithPat`.
    pub fn substitute_with_pat(&mut self, pat: &Pattern, kind: MatchKind) -> bool {
        unsafe { c::FcConfigSubstitute(self.cval(), pat.cval(), kind.to_c()) == FC_TRUE }
    }
}

// =============================================================================
// pattern.zig — the Pattern handle.
// =============================================================================

/// An opaque Fontconfig pattern handle.
// PORT NOTE: `pattern.zig:12-145` `Pattern = opaque {}`. `#[repr(transparent)]`
// over the C `FcPattern`. Only the surface the discovery layer uses is carved
// (the `ObjectIterator`/`ValueIterator`/`filter`/`print`/`delete` helpers are
// not needed by any caller yet).
#[repr(transparent)]
pub struct Pattern(c::FcPattern);

impl Pattern {
    #[inline]
    fn cval(&self) -> *mut c::FcPattern {
        (self as *const Pattern as *mut Pattern).cast()
    }

    /// Create a new, empty pattern.
    // PORT NOTE: `pattern.zig:13-15` `create`.
    pub fn create() -> *mut Pattern {
        unsafe { c::FcPatternCreate().cast() }
    }

    /// Parse a Fontconfig name string (e.g. `":monospace"`) into a pattern.
    // PORT NOTE: `pattern.zig:17-19` `parse`.
    pub fn parse(str: &CStr) -> *mut Pattern {
        unsafe { c::FcNameParse(str.as_ptr().cast()).cast() }
    }

    /// Destroy this pattern.
    // PORT NOTE: `pattern.zig:21-23` `destroy`.
    pub fn destroy(&mut self) {
        unsafe { c::FcPatternDestroy(self.cval()) }
    }

    /// Supply default values for underspecified pattern elements.
    // PORT NOTE: `pattern.zig:25-27` `defaultSubstitute`.
    pub fn default_substitute(&mut self) {
        unsafe { c::FcDefaultSubstitute(self.cval()) }
    }

    /// Add `value` under property `prop`. If `append` is true it is added after
    /// existing values, otherwise before them.
    // PORT NOTE: `pattern.zig:29-36` `add`.
    pub fn add(&mut self, prop: Property, value: Value, append: bool) -> bool {
        unsafe {
            c::FcPatternAdd(
                self.cval(),
                prop.cval().as_ptr(),
                value.cval(),
                if append { FC_TRUE } else { FC_FALSE },
            ) == FC_TRUE
        }
    }

    /// Get the `id`-th value of property `prop`.
    // PORT NOTE: `pattern.zig:38-48` `get`.
    pub fn get(&self, prop: Property, id: u32) -> std::result::Result<Value<'_>, Error> {
        let mut val: c::FcValue = unsafe { std::mem::zeroed() };
        Result::from_c(unsafe {
            c::FcPatternGet(
                self.cval(),
                prop.cval().as_ptr(),
                id as std::os::raw::c_int,
                &mut val,
            )
        })
        .to_error()?;
        Ok(Value::init(&val))
    }
}

// =============================================================================
// char_set.zig — the CharSet handle.
// =============================================================================

/// An opaque Fontconfig character set handle.
// PORT NOTE: `char_set.zig:5-29` `CharSet = opaque {}`.
#[repr(transparent)]
pub struct CharSet(c::FcCharSet);

impl CharSet {
    #[inline]
    fn cval(&self) -> *mut c::FcCharSet {
        (self as *const CharSet as *mut CharSet).cast()
    }

    /// Create a new, empty character set.
    // PORT NOTE: `char_set.zig:6-8` `create`.
    pub fn create() -> *mut CharSet {
        unsafe { c::FcCharSetCreate().cast() }
    }

    /// Destroy this character set.
    // PORT NOTE: `char_set.zig:10-12` `destroy`.
    pub fn destroy(&mut self) {
        unsafe { c::FcCharSetDestroy(self.cval()) }
    }

    /// Add codepoint `cp` to the set.
    // PORT NOTE: `char_set.zig:14-16` `addChar`.
    pub fn add_char(&mut self, cp: u32) -> bool {
        unsafe { c::FcCharSetAddChar(self.cval(), cp) == FC_TRUE }
    }

    /// Whether codepoint `cp` is in the set.
    // PORT NOTE: `char_set.zig:18-20` `hasChar`.
    pub fn has_char(&self, cp: u32) -> bool {
        unsafe { c::FcCharSetHasChar(self.cval(), cp) == FC_TRUE }
    }
}

// =============================================================================
// lang_set.zig — the LangSet handle.
// =============================================================================

/// The `FcLangResult` "equal" value (`c.FcLangEqual`), `0`.
const FC_LANG_EQUAL: c::FcLangResult = c::_FcLangResult_FcLangEqual;

/// An opaque Fontconfig language set handle.
// PORT NOTE: `lang_set.zig:5-29` `LangSet = opaque {}`.
#[repr(transparent)]
pub struct LangSet(c::FcLangSet);

impl LangSet {
    #[inline]
    fn cval(&self) -> *mut c::FcLangSet {
        (self as *const LangSet as *mut LangSet).cast()
    }

    /// Create a new, empty language set.
    // PORT NOTE: `lang_set.zig:6-8` `create`.
    pub fn create() -> *mut LangSet {
        unsafe { c::FcLangSetCreate().cast() }
    }

    /// Destroy this language set.
    // PORT NOTE: `lang_set.zig:10-12` `destroy`.
    pub fn destroy(&mut self) {
        unsafe { c::FcLangSetDestroy(self.cval()) }
    }

    /// Add language `lang` (e.g. `"en-US"`) to the set.
    // PORT NOTE: `lang_set.zig:14-16` `addLang`.
    pub fn add_lang(&mut self, lang: &CStr) -> bool {
        unsafe { c::FcLangSetAdd(self.cval(), lang.as_ptr().cast()) == FC_TRUE }
    }

    /// Whether the set contains an EXACT match for `lang` (`FcLangEqual`, i.e.
    /// same language AND territory — a different territory does NOT match).
    // PORT NOTE: `lang_set.zig:18-20` `hasLang`.
    pub fn has_lang(&self, lang: &CStr) -> bool {
        unsafe { c::FcLangSetHasLang(self.cval(), lang.as_ptr().cast()) == FC_LANG_EQUAL }
    }
}

// =============================================================================
// font_set.zig — the FontSet handle.
// =============================================================================

/// An opaque Fontconfig font set handle.
// PORT NOTE: `font_set.zig:6-35` `FontSet = opaque {}`.
#[repr(transparent)]
pub struct FontSet(c::FcFontSet);

impl FontSet {
    #[inline]
    fn cval(&self) -> *mut c::FcFontSet {
        (self as *const FontSet as *mut FontSet).cast()
    }

    /// Create a new, empty font set.
    // PORT NOTE: `font_set.zig:7-9` `create`.
    pub fn create() -> *mut FontSet {
        unsafe { c::FcFontSetCreate().cast() }
    }

    /// Destroy this font set.
    // PORT NOTE: `font_set.zig:11-13` `destroy`.
    pub fn destroy(&mut self) {
        unsafe { c::FcFontSetDestroy(self.cval()) }
    }

    /// The patterns held by this font set (empty if none).
    // PORT NOTE: `font_set.zig:15-22` `fonts`.
    pub fn fonts(&self) -> &[*mut Pattern] {
        let s = self.cval();
        let fonts = unsafe { (*s).fonts };
        if fonts.is_null() {
            return &[];
        }
        let len = unsafe { (*s).nfont } as usize;
        unsafe { std::slice::from_raw_parts(fonts.cast::<*mut Pattern>(), len) }
    }

    /// Append `pat` to this font set.
    // PORT NOTE: `font_set.zig:24-26` `add`.
    #[allow(dead_code)]
    pub fn add(&mut self, pat: &Pattern) -> bool {
        unsafe { c::FcFontSetAdd(self.cval(), pat.cval()) == FC_TRUE }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // PORT NOTE: init.zig:25-28 `test "version"`.
    #[test]
    fn fc_version() {
        assert!(version() > 0);
    }

    // PORT NOTE: init.zig:30-33 `test "init"`.
    #[test]
    fn fc_init() {
        // `fini()` below tears down fontconfig's process-global state; see
        // `FC_GLOBAL`. Exclusive: no other fontconfig test may be running.
        let _fc = global_exclusive();
        assert!(init());
        fini();
    }

    // PORT NOTE: init.zig:35-38 `test "initLoadConfig"`.
    #[test]
    fn fc_init_load_config() {
        // Shared: fontconfig's global state must not be torn down under us
        // (see `FC_GLOBAL`).
        let _fc = global_shared();
        let config = init_load_config();
        unsafe { (*config).destroy() };
    }

    // PORT NOTE: init.zig:40-43 `test "initLoadConfigAndFonts"`.
    #[test]
    fn fc_init_load_config_and_fonts() {
        // Shared: fontconfig's global state must not be torn down under us
        // (see `FC_GLOBAL`).
        let _fc = global_shared();
        let config = init_load_config_and_fonts();
        unsafe { (*config).destroy() };
    }

    // PORT NOTE: pattern.zig:147-161 `test "create"`.
    #[test]
    fn pattern_create() {
        // Shared: fontconfig's global state must not be torn down under us
        // (see `FC_GLOBAL`).
        let _fc = global_shared();
        let pat = unsafe { &mut *Pattern::create() };
        assert!(pat.add(Property::Family, Value::String(c"monospace"), false));
        assert!(pat.add(Property::Weight, Value::Integer(Weight::Bold as i32), false));
        {
            let val = pat.get(Property::Family, 0).unwrap();
            assert!(matches!(val, Value::String(_)));
            let Value::String(s) = val else {
                unreachable!()
            };
            assert_eq!(s, c"monospace");
        }
        pat.destroy();
    }

    // PORT NOTE: pattern.zig:163-168 `test "name parse"`.
    #[test]
    fn pattern_name_parse() {
        // Shared: fontconfig's global state must not be torn down under us
        // (see `FC_GLOBAL`).
        let _fc = global_shared();
        let pat = unsafe { &mut *Pattern::parse(c":monospace") };
        pat.default_substitute();
        pat.destroy();
    }

    // PORT NOTE: common.zig:110-114 `test "cval"`.
    #[test]
    fn property_cval() {
        assert_eq!(Property::Family.cval(), c"family");
        assert_eq!(Property::PixelSize.cval(), c"pixelsize");
    }

    // PORT NOTE: char_set.zig:31-40 `test "create"`.
    #[test]
    fn char_set_create() {
        // Shared: fontconfig's global state must not be torn down under us
        // (see `FC_GLOBAL`).
        let _fc = global_shared();
        let fs = unsafe { &mut *CharSet::create() };
        assert!(!fs.has_char(0x20));
        assert!(fs.add_char(0x20));
        assert!(fs.has_char(0x20));
        fs.destroy();
    }

    // PORT NOTE: font_set.zig:37-44 `test "create"`.
    #[test]
    fn font_set_create() {
        // Shared: fontconfig's global state must not be torn down under us
        // (see `FC_GLOBAL`).
        let _fc = global_shared();
        let fs = unsafe { &mut *FontSet::create() };
        assert_eq!(fs.fonts().len(), 0);
        fs.destroy();
    }

    // PORT NOTE: lang_set.zig:31-38 `test "create"`.
    #[test]
    fn lang_set_create() {
        // Shared: fontconfig's global state must not be torn down under us
        // (see `FC_GLOBAL`).
        let _fc = global_shared();
        let fs = unsafe { &mut *LangSet::create() };
        assert!(!fs.has_lang(c"und-zsye"));
        fs.destroy();
    }

    // PORT NOTE: lang_set.zig:40-61 `test "hasLang exact match"`.
    #[test]
    fn lang_set_has_lang_exact_match() {
        // Shared: fontconfig's global state must not be torn down under us
        // (see `FC_GLOBAL`).
        let _fc = global_shared();
        // Exact match: langset with "en-US" returns true for "en-US".
        let fs = unsafe { &mut *LangSet::create() };
        assert!(fs.add_lang(c"en-US"));
        assert!(fs.has_lang(c"en-US"));

        // Exact match: langset with "und-zsye" returns true for "und-zsye".
        let fs_emoji = unsafe { &mut *LangSet::create() };
        assert!(fs_emoji.add_lang(c"und-zsye"));
        assert!(fs_emoji.has_lang(c"und-zsye"));

        // Mismatch: langset with "en-US" returns false for "fr".
        assert!(!fs.has_lang(c"fr"));

        // Partial match: langset with "en-US" returns false for "en-GB"
        // (different territory, but we only want exact matches).
        assert!(!fs.has_lang(c"en-GB"));

        fs.destroy();
        fs_emoji.destroy();
    }
}
