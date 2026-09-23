//! Thin, faithful safe wrappers around `fontconfig-sys` — the Rust port of the
//! reference's `pkg/fontconfig/*.zig` binding package (a hand-written safe layer
//! over the Fontconfig C API), sibling of [`crate::ft`] and [`crate::hb`].
//!
//! `pkg/fontconfig` is a carved reference target with its own test contract: all
//! 14 of its NAMED `test` blocks are ported verbatim into this module's
//! `#[cfg(test)] mod tests`, plus a `ref_all_decls` standing in for `main.zig`'s
//! two anonymous blocks (`refAllDecls` + the `_ = @import("test.zig")`
//! re-import) — 16 blocks in the reference, 15 `#[test]`s here (R1). The module
//! mirrors the reference's own exports (`main.zig`): the opaque handle types
//! `Config`/`Pattern`/`CharSet`/`LangSet`/`FontSet`/`ObjectSet`/`Range`, the
//! `Matrix` value struct, the `Weight`/`Slant`/`Spacing`/`Property`/`Result`/
//! `MatchKind` enums, the full 11-tag `Type`/`Value` model plus `ValueBinding`,
//! the `Error` set, and the `init.zig` free functions.
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
// PORT NOTE: `value.zig:9-21` `Type = enum(c_int)`.
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
// PORT NOTE: `value.zig:23-70` `Value = union(Type)` — the full 11-tag union. The
// lifetime `'a` ties a borrowed [`Value::String`] to the pattern it was read from
// (the reference's `[:0]const u8` slice into pattern-owned memory); the pointer
// payloads are likewise borrowed from the pattern, exactly as the reference's
// `*const Matrix`/`*const CharSet`/`*const LangSet`/`*const Range` are.
#[derive(Clone, Copy, Debug)]
pub enum Value<'a> {
    Unknown,
    Void,
    Integer(i32),
    Double(f64),
    String(&'a CStr),
    Bool(bool),
    Matrix(*const Matrix),
    CharSet(*const CharSet),
    FtFace(*mut std::os::raw::c_void),
    LangSet(*const LangSet),
    Range(*const Range),
}

impl<'a> Value<'a> {
    /// The active tag of this value.
    // PORT NOTE: `std.meta.activeTag(self)` on the `union(Type)`.
    pub fn tag(self) -> Type {
        match self {
            Value::Unknown => Type::Unknown,
            Value::Void => Type::Void,
            Value::Integer(_) => Type::Integer,
            Value::Double(_) => Type::Double,
            Value::String(_) => Type::String,
            Value::Bool(_) => Type::Bool,
            Value::Matrix(_) => Type::Matrix,
            Value::CharSet(_) => Type::CharSet,
            Value::FtFace(_) => Type::FtFace,
            Value::LangSet(_) => Type::LangSet,
            Value::Range(_) => Type::Range,
        }
    }

    /// Read a raw `FcValue` into a [`Value`].
    // PORT NOTE: `value.zig:36-50` `init`. The reference's `@enumFromInt(cvalue.type)`
    // is illegal behavior for a tag outside the 11; Fontconfig never produces one, and
    // Rust cannot construct such a `Type` at all, so an out-of-range tag reads as
    // `.unknown` — the same "no usable value" outcome.
    pub fn init(cvalue: &c::FcValue) -> Value<'a> {
        match cvalue.type_ {
            c::_FcType_FcTypeVoid => Value::Void,
            c::_FcType_FcTypeString => Value::String(unsafe { CStr::from_ptr(cvalue.u.s.cast()) }),
            c::_FcType_FcTypeInteger => Value::Integer(unsafe { cvalue.u.i }),
            c::_FcType_FcTypeDouble => Value::Double(unsafe { cvalue.u.d }),
            c::_FcType_FcTypeBool => Value::Bool(unsafe { cvalue.u.b } == FC_TRUE),
            c::_FcType_FcTypeMatrix => Value::Matrix(unsafe { cvalue.u.m }.cast()),
            c::_FcType_FcTypeCharSet => Value::CharSet(unsafe { cvalue.u.c }.cast()),
            c::_FcType_FcTypeFTFace => Value::FtFace(unsafe { cvalue.u.f }),
            c::_FcType_FcTypeLangSet => Value::LangSet(unsafe { cvalue.u.l }.cast()),
            c::_FcType_FcTypeRange => Value::Range(unsafe { cvalue.u.r }.cast()),
            _ => Value::Unknown,
        }
    }

    /// Build a raw `FcValue` from this [`Value`].
    // PORT NOTE: `value.zig:52-69` `cval`. The reference leaves the `u` payload
    // `undefined` for `.unknown`/`.void`; rustty zeroes it (Rust has no `undefined`,
    // and a zeroed union is what a C caller of `FcValue{ .type = FcTypeVoid }`
    // would build).
    pub fn cval(self) -> c::FcValue {
        match self {
            Value::Unknown => c::FcValue {
                type_: c::_FcType_FcTypeUnknown,
                u: unsafe { std::mem::zeroed() },
            },
            Value::Void => c::FcValue {
                type_: c::_FcType_FcTypeVoid,
                u: unsafe { std::mem::zeroed() },
            },
            Value::Integer(v) => c::FcValue {
                type_: c::_FcType_FcTypeInteger,
                u: c::_FcValue__bindgen_ty_1 { i: v },
            },
            Value::Double(v) => c::FcValue {
                type_: c::_FcType_FcTypeDouble,
                u: c::_FcValue__bindgen_ty_1 { d: v },
            },
            Value::String(v) => c::FcValue {
                type_: c::_FcType_FcTypeString,
                u: c::_FcValue__bindgen_ty_1 {
                    s: v.as_ptr().cast(),
                },
            },
            Value::Bool(v) => c::FcValue {
                type_: c::_FcType_FcTypeBool,
                u: c::_FcValue__bindgen_ty_1 {
                    b: if v { FC_TRUE } else { FC_FALSE },
                },
            },
            Value::Matrix(v) => c::FcValue {
                type_: c::_FcType_FcTypeMatrix,
                u: c::_FcValue__bindgen_ty_1 { m: v.cast() },
            },
            Value::CharSet(v) => c::FcValue {
                type_: c::_FcType_FcTypeCharSet,
                u: c::_FcValue__bindgen_ty_1 { c: v.cast() },
            },
            Value::FtFace(v) => c::FcValue {
                type_: c::_FcType_FcTypeFTFace,
                u: c::_FcValue__bindgen_ty_1 { f: v },
            },
            Value::LangSet(v) => c::FcValue {
                type_: c::_FcType_FcTypeLangSet,
                u: c::_FcValue__bindgen_ty_1 { l: v.cast() },
            },
            Value::Range(v) => c::FcValue {
                type_: c::_FcType_FcTypeRange,
                u: c::_FcValue__bindgen_ty_1 { r: v.cast() },
            },
        }
    }
}

/// How a value is bound to a pattern element.
// PORT NOTE: `value.zig:72-76` `ValueBinding = enum(c_int)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValueBinding {
    Weak,
    Strong,
    Same,
}

impl ValueBinding {
    /// Map a raw `FcValueBinding` to this enum.
    // PORT NOTE: the reference's `@enumFromInt(binding)`. `FcValueBindingEnd`
    // (the C enum's `INT_MAX` padding sentinel) is not a real binding and is not
    // in the reference enum; it reads as `.same`, Fontconfig's own "inherit the
    // previous binding" value.
    pub fn from_c(v: c::FcValueBinding) -> ValueBinding {
        match v {
            c::_FcValueBinding_FcValueBindingWeak => ValueBinding::Weak,
            c::_FcValueBinding_FcValueBindingStrong => ValueBinding::Strong,
            _ => ValueBinding::Same,
        }
    }

    /// `@intFromEnum(binding)`.
    pub fn to_c(self) -> c::FcValueBinding {
        match self {
            ValueBinding::Weak => c::_FcValueBinding_FcValueBindingWeak,
            ValueBinding::Strong => c::_FcValueBinding_FcValueBindingStrong,
            ValueBinding::Same => c::_FcValueBinding_FcValueBindingSame,
        }
    }
}

// =============================================================================
// matrix.zig — the Matrix value struct.
// =============================================================================

/// A Fontconfig 2x2 transformation matrix.
// PORT NOTE: `matrix.zig:5-10` `Matrix = extern struct`. `#[repr(C)]` gives the
// same layout as the C `FcMatrix` the reference `@ptrCast`s to.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Matrix {
    pub xx: f64,
    pub xy: f64,
    pub yx: f64,
    pub yy: f64,
}

// =============================================================================
// range.zig — the Range handle.
// =============================================================================

/// An opaque Fontconfig range handle.
// PORT NOTE: `range.zig:5-13` `Range = opaque {}`.
#[repr(transparent)]
pub struct Range(c::FcRange);

impl Range {
    #[inline]
    fn cval(&self) -> *mut c::FcRange {
        (self as *const Range as *mut Range).cast()
    }

    /// Destroy this range.
    // PORT NOTE: `range.zig:6-8` `destroy`.
    pub fn destroy(&mut self) {
        unsafe { c::FcRangeDestroy(self.cval()) }
    }
}

// =============================================================================
// object_set.zig — the ObjectSet handle.
// =============================================================================

/// An opaque Fontconfig object (property-name) set handle.
// PORT NOTE: `object_set.zig:5-21` `ObjectSet = opaque {}`.
#[repr(transparent)]
pub struct ObjectSet(c::FcObjectSet);

impl ObjectSet {
    #[inline]
    fn cval(&self) -> *mut c::FcObjectSet {
        (self as *const ObjectSet as *mut ObjectSet).cast()
    }

    /// Create a new, empty object set.
    // PORT NOTE: `object_set.zig:6-8` `create`.
    pub fn create() -> *mut ObjectSet {
        unsafe { c::FcObjectSetCreate().cast() }
    }

    /// Destroy this object set.
    // PORT NOTE: `object_set.zig:10-12` `destroy`.
    pub fn destroy(&mut self) {
        unsafe { c::FcObjectSetDestroy(self.cval()) }
    }

    /// Add property `p` to the set.
    // PORT NOTE: `object_set.zig:14-16` `add`.
    pub fn add(&mut self, p: Property) -> bool {
        unsafe { c::FcObjectSetAdd(self.cval(), p.cval().as_ptr()) == FC_TRUE }
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

    /// Select fonts matching `pat`, creating patterns holding only the
    /// properties listed in `os`.
    // PORT NOTE: `config.zig:16-18` `fontList`.
    pub fn font_list(&mut self, pat: &Pattern, os: &ObjectSet) -> *mut FontSet {
        unsafe { c::FcFontList(self.cval(), pat.cval(), os.cval()).cast() }
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
// over the C `FcPattern`.
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

    /// Delete every value of property `prop` from this pattern.
    // PORT NOTE: `pattern.zig:50-52` `delete`.
    pub fn delete(&mut self, prop: Property) -> bool {
        unsafe { c::FcPatternDel(self.cval(), prop.cval().as_ptr()) == FC_TRUE }
    }

    /// Return a NEW pattern holding only the properties listed in `os`.
    // PORT NOTE: `pattern.zig:54-56` `filter`. The returned pattern is owned by
    // the caller (`destroy` it), like every other constructor here.
    pub fn filter(&mut self, os: &ObjectSet) -> *mut Pattern {
        unsafe { c::FcPatternFilter(self.cval(), os.cval()).cast() }
    }

    /// An iterator over this pattern's objects (properties).
    // PORT NOTE: `pattern.zig:58-60` `objectIterator`.
    pub fn object_iterator(&mut self) -> ObjectIterator {
        ObjectIterator {
            pat: self.cval(),
            iter: None,
        }
    }

    /// Print this pattern to stdout (debugging aid).
    // PORT NOTE: `pattern.zig:62-64` `print`.
    pub fn print(&mut self) {
        unsafe { c::FcPatternPrint(self.cval()) }
    }
}

/// An iterator over a pattern's objects (properties).
// PORT NOTE: `pattern.zig:70-114` `Pattern.ObjectIterator`. Kept a module-level
// type (Rust has no nested types inside an `impl`); `Pattern::object_iterator` is
// still the only constructor, exactly as in the reference.
pub struct ObjectIterator {
    // pat: *c.struct__FcPattern,
    pat: *mut c::FcPattern,
    // iter: ?c.struct__FcPatternIter,
    iter: Option<c::FcPatternIter>,
}

impl ObjectIterator {
    /// Move to the next object, returns true if there is another
    /// object and false otherwise. If this is the first call, this
    /// will be the first object.
    // PORT NOTE: `pattern.zig:76-96` `next`. Named `next` to match the reference
    // iterator; it is not `Iterator::next` (it yields no item — the payload is read
    // through `object`/`value_len`/`value_iterator`).
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> bool {
        // Null means our first iterator
        if self.iter.is_none() {
            // If we have no objects, do not create iterator
            if unsafe { c::FcPatternObjectCount(self.pat) } == 0 {
                return false;
            }

            let mut iter: c::FcPatternIter = unsafe { std::mem::zeroed() };
            unsafe { c::FcPatternIterStart(self.pat, &mut iter) };
            // assert(c.FcPatternIterIsValid(self.pat, &iter) == c.FcTrue);
            assert!(unsafe { c::FcPatternIterIsValid(self.pat, &mut iter) } == FC_TRUE);
            self.iter = Some(iter);

            // Return right away because the fontconfig iterator pattern
            // is do/while.
            return true;
        }

        unsafe { c::FcPatternIterNext(self.pat, self.iter.as_mut().unwrap()) == FC_TRUE }
    }

    /// The current object's name.
    // PORT NOTE: `pattern.zig:98-103` `object`. Returns the BYTE slice of the
    // NUL-terminated fontconfig object name (the reference's `sliceTo(..., 0)`),
    // never a lossy `String`.
    pub fn object(&mut self) -> &[u8] {
        let p = unsafe { c::FcPatternIterGetObject(self.pat, self.iter.as_mut().unwrap()) };
        unsafe { CStr::from_ptr(p) }.to_bytes()
    }

    /// How many values the current object has.
    // PORT NOTE: `pattern.zig:105-107` `valueLen`.
    pub fn value_len(&mut self) -> usize {
        unsafe { c::FcPatternIterValueCount(self.pat, self.iter.as_mut().unwrap()) as usize }
    }

    /// An iterator over the current object's values.
    // PORT NOTE: `pattern.zig:109-113` `valueIterator`.
    pub fn value_iterator(&mut self) -> ValueIterator<'_> {
        let iter: *mut c::FcPatternIter = self.iter.as_mut().unwrap();
        ValueIterator {
            pat: self.pat,
            iter,
            max: unsafe { c::FcPatternIterValueCount(self.pat, iter) },
            id: 0,
            _marker: std::marker::PhantomData,
        }
    }
}

/// One value of a pattern object.
// PORT NOTE: `pattern.zig:123-127` `Pattern.ValueIterator.Entry`.
#[derive(Clone, Copy, Debug)]
pub struct Entry<'a> {
    pub result: Result,
    pub value: Value<'a>,
    pub binding: ValueBinding,
}

/// An iterator over one pattern object's values.
// PORT NOTE: `pattern.zig:116-142` `Pattern.ValueIterator`. The reference holds a
// bare `*c.struct__FcPatternIter` into its parent `ObjectIterator`; the `'a`
// lifetime here is that same borrow, made explicit so the parent cannot be
// advanced (or dropped) while this iterator is alive.
pub struct ValueIterator<'a> {
    pat: *mut c::FcPattern,
    iter: *mut c::FcPatternIter,
    max: std::os::raw::c_int,
    id: std::os::raw::c_int,
    _marker: std::marker::PhantomData<&'a mut ObjectIterator>,
}

impl<'a> ValueIterator<'a> {
    // PORT NOTE: `pattern.zig:129-141` `next`. Named `next` to match the reference;
    // not `Iterator::next`, whose item type could not borrow from `self`.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Option<Entry<'a>> {
        if self.id >= self.max {
            return None;
        }
        let mut value: c::FcValue = unsafe { std::mem::zeroed() };
        let mut binding: c::FcValueBinding = c::_FcValueBinding_FcValueBindingWeak;
        let result = unsafe {
            c::FcPatternIterGetValue(self.pat, self.iter, self.id, &mut value, &mut binding)
        };
        self.id += 1;

        Some(Entry {
            result: Result::from_c(result),
            binding: ValueBinding::from_c(binding),
            value: Value::init(&value),
        })
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
    pub fn add(&mut self, pat: &Pattern) -> bool {
        unsafe { c::FcFontSetAdd(self.cval(), pat.cval()) == FC_TRUE }
    }

    /// Print this font set to stdout (debugging aid).
    // PORT NOTE: `font_set.zig:28-30` `print`.
    pub fn print(&mut self) {
        unsafe { c::FcFontSetPrint(self.cval()) }
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

    // PORT NOTE: object_set.zig:23-30 `test "create"`.
    #[test]
    fn object_set_create() {
        // Shared: fontconfig's global state must not be torn down under us
        // (see `FC_GLOBAL`).
        let _fc = global_shared();
        let os = unsafe { &mut *ObjectSet::create() };
        assert!(os.add(Property::Family));
        os.destroy();
    }

    // PORT NOTE: test.zig:4-22 `test "fc-list"`.
    #[test]
    fn fc_list() {
        // Shared: fontconfig's global state must not be torn down under us
        // (see `FC_GLOBAL`).
        let _fc = global_shared();

        // var cfg = fontconfig.initLoadConfigAndFonts(); defer cfg.destroy();
        let cfg = unsafe { &mut *init_load_config_and_fonts() };
        // var pat = fontconfig.Pattern.create(); defer pat.destroy();
        let pat = unsafe { &mut *Pattern::create() };
        // var os = fontconfig.ObjectSet.create(); defer os.destroy();
        let os = unsafe { &mut *ObjectSet::create() };

        // var fs = cfg.fontList(pat, os); defer fs.destroy();
        let fs = unsafe { &mut *cfg.font_list(pat, os) };

        // Note: this is environmental, but in general we expect all our
        // testing environments to have at least one font.
        assert!(!fs.fonts().is_empty());

        fs.destroy();
        os.destroy();
        pat.destroy();
        cfg.destroy();
    }

    // PORT NOTE: test.zig:24-66 `test "fc-match"`.
    #[test]
    fn fc_match() {
        // Shared: fontconfig's global state must not be torn down under us
        // (see `FC_GLOBAL`).
        let _fc = global_shared();

        // var cfg = fontconfig.initLoadConfigAndFonts(); defer cfg.destroy();
        let cfg = unsafe { &mut *init_load_config_and_fonts() };

        // var pat = fontconfig.Pattern.create(); errdefer pat.destroy();
        let pat = unsafe { &mut *Pattern::create() };
        // try testing.expect(cfg.substituteWithPat(pat, .pattern));
        assert!(cfg.substitute_with_pat(pat, MatchKind::Pattern));
        // pat.defaultSubstitute();
        pat.default_substitute();

        // const result = cfg.fontSort(pat, false, null);
        let result = cfg.font_sort(pat, false, None);

        // var fs = fontconfig.FontSet.create(); defer fs.destroy();
        let fs = unsafe { &mut *FontSet::create() };

        {
            // const fonts = result.fs.fonts();
            let fonts = unsafe { (*result.fs).fonts() }.to_vec();
            // try testing.expect(fonts.len > 0);
            assert!(!fonts.is_empty());
            for font in fonts {
                // const pat_prep = try cfg.fontRenderPrepare(pat, font);
                let pat_prep = cfg.font_render_prepare(pat, unsafe { &*font }).unwrap();
                // try testing.expect(fs.add(pat_prep));
                assert!(fs.add(unsafe { &*pat_prep }));
            }
            // result.fs.destroy();
            unsafe { (*result.fs).destroy() };
            // pat.destroy();
            pat.destroy();
        }

        {
            // for (fs.fonts()) |font| { var it = font.objectIterator(); ... }
            for &font in fs.fonts() {
                let mut it = unsafe { (*font).object_iterator() };
                while it.next() {
                    // try testing.expect(it.object().len > 0);
                    assert!(!it.object().is_empty());
                    // try testing.expect(it.valueLen() > 0);
                    assert!(it.value_len() > 0);
                    // var value_it = it.valueIterator();
                    let mut value_it = it.value_iterator();
                    // while (value_it.next()) |entry| { ... }
                    while let Some(entry) = value_it.next() {
                        // try testing.expect(entry.value != .unknown);
                        assert_ne!(Type::Unknown, entry.value.tag());
                    }
                }
            }
        }

        // defer fs.destroy();
        // defer for (fs.fonts()) |font| font.destroy();
        //
        // UPSTREAM BUG — NOT PORTED AS WRITTEN (P4-U3, .port/phase-4/
        // UPSTREAM_BUGS.md; reference defect (R13), UPSTREAM-DEFECTS.md UD109,
        // re-verified at the pin by task R13-D): the reference's defer PAIR
        // double-destroys every
        // member — `FcFontSetDestroy` itself calls `FcPatternDestroy` on each
        // pattern in the set, so the per-font `destroy()` loop drops each
        // refcount to zero (freeing the pattern) and the subsequent
        // `fs.destroy()` then re-destroys freed patterns. glibc's allocator
        // hardening catches it deterministically on Fedora ("corrupted size
        // vs. prev_size"); Zig CI tolerates the UAF by luck. The
        // fontconfig-documented teardown is `FcFontSetDestroy` alone.
        fs.destroy();
        cfg.destroy();
    }

    // PORT NOTE: `main.zig:39-41` `test { refAllDecls(@This()); }` (and `:43-45`,
    // the `_ = @import("test.zig")` re-import, whose blocks are the two tests
    // above). Mirrors the crate's `ref_all_decls` convention (`face/mod.rs`):
    // every declaration `main.zig` re-exports is reified so a signature drift is
    // a compile error.
    #[test]
    fn ref_all_decls() {
        let _ = std::mem::size_of::<CharSet>();
        let _ = std::mem::size_of::<Config>();
        let _ = std::mem::size_of::<Error>();
        let _ = std::mem::size_of::<FontSet>();
        let _ = std::mem::size_of::<LangSet>();
        let _ = std::mem::size_of::<Matrix>();
        let _ = std::mem::size_of::<ObjectSet>();
        let _ = std::mem::size_of::<Pattern>();
        let _ = std::mem::size_of::<Range>();
        let _ = std::mem::size_of::<FontSortResult>();

        let _ = init as fn() -> bool;
        let _ = fini as fn();
        let _ = init_load_config as fn() -> *mut Config;
        let _ = init_load_config_and_fonts as fn() -> *mut Config;
        let _ = version as fn() -> u32;

        let _ = Config::destroy as fn(&mut Config);
        let _ = Config::font_list as fn(&mut Config, &Pattern, &ObjectSet) -> *mut FontSet;
        let _ = Config::font_sort
            as fn(&mut Config, &Pattern, bool, Option<*mut *mut CharSet>) -> FontSortResult;
        let _ = Config::substitute_with_pat as fn(&mut Config, &Pattern, MatchKind) -> bool;

        let _ = Pattern::create as fn() -> *mut Pattern;
        let _ = Pattern::parse as fn(&CStr) -> *mut Pattern;
        let _ = Pattern::destroy as fn(&mut Pattern);
        let _ = Pattern::default_substitute as fn(&mut Pattern);
        let _ = Pattern::delete as fn(&mut Pattern, Property) -> bool;
        let _ = Pattern::filter as fn(&mut Pattern, &ObjectSet) -> *mut Pattern;
        let _ = Pattern::print as fn(&mut Pattern);
        let _ = Pattern::object_iterator as fn(&mut Pattern) -> ObjectIterator;
        let _ = ObjectIterator::next as fn(&mut ObjectIterator) -> bool;
        let _ = ObjectIterator::value_len as fn(&mut ObjectIterator) -> usize;

        let _ = ObjectSet::create as fn() -> *mut ObjectSet;
        let _ = ObjectSet::destroy as fn(&mut ObjectSet);
        let _ = ObjectSet::add as fn(&mut ObjectSet, Property) -> bool;
        let _ = Range::destroy as fn(&mut Range);
        let _ = FontSet::create as fn() -> *mut FontSet;
        let _ = FontSet::destroy as fn(&mut FontSet);
        let _ = FontSet::add as fn(&mut FontSet, &Pattern) -> bool;
        let _ = FontSet::print as fn(&mut FontSet);
        let _ = CharSet::create as fn() -> *mut CharSet;
        let _ = LangSet::create as fn() -> *mut LangSet;
        let _ = Value::<'static>::tag as fn(Value<'static>) -> Type;
        let _ = Value::<'static>::cval as fn(Value<'static>) -> c::FcValue;
        let _ = ValueBinding::from_c as fn(c::FcValueBinding) -> ValueBinding;
        let _ = ValueBinding::to_c as fn(ValueBinding) -> c::FcValueBinding;

        // Every `Value` variant round-trips its tag through `cval`/`init`.
        let matrix = Matrix {
            xx: 1.0,
            xy: 0.0,
            yx: 0.0,
            yy: 1.0,
        };
        let cases: [Value<'_>; 11] = [
            Value::Unknown,
            Value::Void,
            Value::Integer(7),
            Value::Double(1.5),
            Value::String(c"mono"),
            Value::Bool(true),
            Value::Matrix(&matrix),
            Value::CharSet(std::ptr::null()),
            Value::FtFace(std::ptr::null_mut()),
            Value::LangSet(std::ptr::null()),
            Value::Range(std::ptr::null()),
        ];
        for v in cases {
            let cv = v.cval();
            assert_eq!(v.tag(), Value::init(&cv).tag());
        }
        // The payloads survive the round-trip too.
        let cv = Value::Integer(7).cval();
        assert!(matches!(Value::init(&cv), Value::Integer(7)));
        let cv = Value::Double(1.5).cval();
        assert!(matches!(Value::init(&cv), Value::Double(d) if d == 1.5));
        let cv = Value::Bool(true).cval();
        assert!(matches!(Value::init(&cv), Value::Bool(true)));
        let cv = Value::String(c"mono").cval();
        assert!(matches!(Value::init(&cv), Value::String(s) if s == c"mono"));
        let cv = Value::Matrix(&matrix).cval();
        let Value::Matrix(m) = Value::init(&cv) else {
            unreachable!()
        };
        assert_eq!(matrix, unsafe { *m });

        // `ValueBinding` round-trips both ways.
        for b in [ValueBinding::Weak, ValueBinding::Strong, ValueBinding::Same] {
            assert_eq!(b, ValueBinding::from_c(b.to_c()));
        }
    }
}
