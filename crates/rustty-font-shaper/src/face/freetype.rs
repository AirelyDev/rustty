//! Face represents a single font face. A single font face has a single set
//! of properties associated with it such as style, weight, etc.
//!
//! A Face isn't typically meant to be used directly. It is usually used
//! via a Family in order to store it in an Atlas.

// const std = @import("std");
// const builtin = @import("builtin");
// const freetype = @import("freetype");
// const harfbuzz = @import("harfbuzz");
// const stb = @import("../../stb/main.zig");
// const assert = @import("../../quirks.zig").inlineAssert;
// const testing = std.testing;
// const Allocator = std.mem.Allocator;
// const font = @import("../main.zig");
// const Glyph = font.Glyph;
// const Library = font.Library;
// const opentype = @import("../opentype.zig");
// const quirks = @import("../../quirks.zig");
// const config = @import("../../config.zig");
//
// const F26Dot6 = opentype.sfnt.F26Dot6;
//
// const log = std.log.scoped(.font_face);
//
// PORT NOTE (imports): the reference imports the `freetype`/`harfbuzz` packages
// and `stb`; here they are the thin FFI-glue modules `crate::ft`, `crate::hb`,
// and `crate::stb` (see those modules). `font.Library` is `crate::library::
// Library`. `font.Glyph`/`font.Atlas`/`font.Metrics`/`opentype.sfnt.F26Dot6` and
// the glyph render options are owned by the PURE `rustty-font` leaf below this
// tier. `assert = quirks.inlineAssert` is `rustty_util::quirks::inlineAssert`.
// `config` supplies `FreetypeLoadFlags` (see `FreetypeLoadFlags` below). `log` is
// a scoped logger; it maps onto the `log` facade crate with the reference's
// `std.log.scoped` tag carried as `target: LOG_SCOPE`
// (`.port/parity-closure/DECISION-logging.md` §6). Reference `log.*` calls are
// kept as verbatim comments (R1) with the live emission beneath them.
use allocator_api2::alloc::Allocator;
/// The reference's `std.log.scoped(.font_face)` tag, carried through the `log`
/// facade's `target:`. `face/freetype.zig`, `face/coretext.zig` and
/// `face/web_canvas.zig` all share this ONE scope in the reference; that is the
/// reference's own choice and is ported as-is.
const LOG_SCOPE: &str = "font_face";

/// The reference guards its variation-axes dump with
/// `comptime builtin.mode == .Debug`; `cfg!(debug_assertions)` is the direct
/// Rust analog (same choice as `face/coretext.rs`).
const DEBUG_MODE: bool = cfg!(debug_assertions);

use rustty_font::atlas::{Atlas, Format};
use rustty_font::glyph::{ConstraintSize, Glyph, RenderOptions, Size};
use rustty_font::metrics::FaceMetrics;
use rustty_font::opentype::sfnt::F26Dot6;
use rustty_util::quirks::inlineAssert as assert;
use std::os::raw::c_int;

use crate::ft::{self, Encoding, LoadFlags, RenderMode, Tag, Target};
use crate::hb;
// PORT NOTE: the reference `const Library = font.Library` resolves to
// `FreetypeLibrary` on every freetype-active backend. Since both backend faces
// are compiled in rustty regardless of the active backend (unlike Zig, which only
// analyzes the selected face), the freetype face names `FreetypeLibrary` directly
// — it always requires a real FreeType library — rather than the backend-switched
// `crate::library::Library` alias (which is `NoopLibrary` on macOS).
use crate::library::{self, FreetypeLibrary as Library};

use freetype_sys as c;

use super::DesiredSize;

/// The error set for the freetype face.
// PORT NOTE: the reference error set is the union of `freetype.Error`,
// `harfbuzz.Error`, `Allocator.Error`, the `Atlas` error, and the local errors
// (`BitmapHandlingError`/`GlyphResizeFailed`/`WrongAtlas`/`UnsupportedGlyphFormat`).
// It is modeled here by one enum with `From` conversions so `?` composes them.
#[derive(Debug)]
pub enum Error {
    Freetype(ft::FtError),
    HarfbuzzFailed,
    OutOfMemory,
    AtlasFull,
    BitmapHandlingError,
    GlyphResizeFailed,
    WrongAtlas,
    UnsupportedGlyphFormat,
    /// Only used by the bitmap-font tests' `glyphIndex(...) orelse return
    /// error.GlyphNotFound`.
    GlyphNotFound,
}

impl From<ft::FtError> for Error {
    fn from(e: ft::FtError) -> Self {
        Error::Freetype(e)
    }
}
impl From<hb::HarfbuzzFailed> for Error {
    fn from(_: hb::HarfbuzzFailed) -> Self {
        Error::HarfbuzzFailed
    }
}
impl From<rustty_font::atlas::OutOfMemory> for Error {
    fn from(_: rustty_font::atlas::OutOfMemory) -> Self {
        Error::OutOfMemory
    }
}
impl From<rustty_font::atlas::ReserveError> for Error {
    fn from(e: rustty_font::atlas::ReserveError) -> Self {
        match e {
            rustty_font::atlas::ReserveError::OutOfMemory => Error::OutOfMemory,
            rustty_font::atlas::ReserveError::AtlasFull => Error::AtlasFull,
        }
    }
}

/// These are the flags to customize how freetype loads fonts.
// PORT NOTE: the reference's `load_flags` field has type `font.face.
// FreetypeLoadFlags`, which in a freetype-active build (the only build in which
// this file is compiled) resolves to `config.FreetypeLoadFlags`. That config type
// lives in the higher `rustty-config` crate (break #9: config injected via DI), so
// it is NOT a dependency of this tier; this is a faithful local stand-in with the
// same fields and defaults (`src/config/Config.zig`'s `FreetypeLoadFlags`), wired
// to the real config type when config DI lands.
// PORT NOTE (`Hash`): `SharedGridSet.Key.hash` feeds this value to
// `std.hash.autoHash`, which walks a packed struct field-by-field. The derive
// reproduces that; without it two grids differing ONLY in load flags collide on
// the same cache key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FreetypeLoadFlags {
    pub hinting: bool,
    // PORT NOTE: Zig field `@"force-autohint"`.
    pub force_autohint: bool,
    pub monochrome: bool,
    pub autohint: bool,
    pub light: bool,
}

impl FreetypeLoadFlags {
    /// The reference's field defaults, reachable as a `const` so the
    /// `face::FREETYPE_LOAD_FLAGS_DEFAULT` constant (`face/mod.rs`'s port of
    /// `freetype_load_flags_default`) can be built at compile time on the
    /// freetype-active backends. `Default::default()` returns this same value.
    pub const DEFAULT: Self = FreetypeLoadFlags {
        hinting: true,
        force_autohint: false,
        monochrome: false,
        autohint: true,
        light: true,
    };
}

impl Default for FreetypeLoadFlags {
    // The defaults here match FreeType's own defaults (see the config comment).
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Options for initializing a font face.
// PORT NOTE: the reference's `font.face.Options` (the shared selector type) has
// `freetype_load_flags: FreetypeLoadFlags`, which is `void` on non-freetype
// backends. In a freetype-active build it is the config stand-in above; this local
// `Options` carries that concrete type so the face is testable on any host.
#[derive(Clone, Copy, Debug)]
pub struct Options {
    pub size: DesiredSize,
    pub freetype_load_flags: FreetypeLoadFlags,
}

/// Synthetic style state applied to the face.
// PORT NOTE: the reference's anonymous `synthetic: packed struct { italic, bold }`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Synthetic {
    pub italic: bool,
    pub bold: bool,
}

pub struct Face {
    // comptime {
    //     // If we have the freetype backend, we should have load flags.
    //     assert(font.face.FreetypeLoadFlags != void);
    // }
    //
    // PORT NOTE: the comptime assertion that `FreetypeLoadFlags` is not `void`
    // is a compile-time guarantee here: `FreetypeLoadFlags` above is a concrete
    // struct (never `()`), so the invariant holds structurally.
    /// Our Library
    pub lib: Library,

    /// Our font face.
    pub face: ft::Face,

    /// This mutex MUST be held while doing anything with the
    /// glyph slot on the freetype face, because this struct
    /// may be shared across multiple surfaces.
    ///
    /// This means that anywhere where `self.face.loadGlyph`
    /// is called, this mutex must be held.
    pub ft_mutex: *mut library::Mutex,

    /// Harfbuzz font corresponding to this face.
    pub hb_font: hb::Font,

    /// Freetype load flags for this font face.
    pub load_flags: FreetypeLoadFlags,

    /// Set quirks.disableDefaultFontFeatures
    pub quirks_disable_default_font_features: bool,

    /// Set to true to apply a synthetic italic to the face.
    pub synthetic: Synthetic,

    /// The current size this font is set to.
    pub size: DesiredSize,
}

impl Face {
    /// Initialize a new font face with the given source in-memory.
    // PORT NOTE: `initFile`.
    pub fn init_file(
        lib: Library,
        path: &std::ffi::CStr,
        index: i32,
        opts: Options,
    ) -> Result<Face, Error> {
        let _guard = unsafe { (*lib.mutex).lock().unwrap() };
        let face = lib.lib.init_face(path, index)?;
        // errdefer face.deinit();
        match Face::init_face(lib, face, opts) {
            Ok(f) => Ok(f),
            Err(e) => {
                face.deinit();
                Err(e)
            }
        }
    }

    /// Initialize a new font face with the given source in-memory.
    // PORT NOTE: `init`.
    pub fn init(lib: Library, source: &[u8], opts: Options) -> Result<Face, Error> {
        let _guard = unsafe { (*lib.mutex).lock().unwrap() };
        let face = lib.lib.init_memory_face(source, 0)?;
        // errdefer face.deinit();
        match Face::init_face(lib, face, opts) {
            Ok(f) => Ok(f),
            Err(e) => {
                face.deinit();
                Err(e)
            }
        }
    }

    // PORT NOTE: `initFace`.
    fn init_face(lib: Library, face: ft::Face, opts: Options) -> Result<Face, Error> {
        Face::init_face_with(lib, face, opts, |f| f.log_variation_axes(DEBUG_MODE))
    }

    /// `initFace` with its one fallible diagnostic step as a parameter.
    ///
    /// PORT NOTE (task R13-P, N53): the seam exists so a test can drive the
    /// reference's `errdefer` arms (`face/freetype.zig:100-105` covering `try
    /// result.name` / `try face.getMMVar()` at `:120-126`) without a font that
    /// makes FreeType fail. It is a plain function parameter, NOT a test-only
    /// hook: production passes its own closure and no test code compiles into
    /// the library.
    fn init_face_with<F>(
        lib: Library,
        face: ft::Face,
        opts: Options,
        log_axes: F,
    ) -> Result<Face, Error>
    where
        F: FnOnce(&Face) -> Result<(), Error>,
    {
        face.select_charmap(Encoding::Unicode)?;
        set_size_(face, opts.size)?;

        let hb_font = hb::create_font(face.handle)?;
        // errdefer hb_font.destroy();
        //
        // PORT NOTE (task R13-P, N53): the old comment here claimed no fallible
        // operation followed. `log_variation_axes` below IS fallible (the
        // reference's `try result.name(&buf)` and `try face.getMMVar()`), so
        // both this errdefer and the `ft_mutex` one below are live. `Face` has
        // no `Drop`, so they are realized explicitly on that error arm.

        // const ft_mutex = try lib.alloc.create(std.Thread.Mutex);
        // errdefer lib.alloc.destroy(ft_mutex);
        // ft_mutex.* = .{};
        let ft_mutex = Box::into_raw(Box::new(library::Mutex::new(())));

        let mut result = Face {
            lib,
            face,
            hb_font,
            ft_mutex,
            load_flags: opts.freetype_load_flags,
            quirks_disable_default_font_features: false,
            synthetic: Synthetic::default(),
            size: opts.size,
        };
        result.quirks_disable_default_font_features = disable_default_font_features(&result);

        // In debug mode, we output information about available variation axes,
        // if they exist.
        //
        // if (comptime builtin.mode == .Debug) mm: { ... }
        //
        // Ported live (S91-F8): the body is `log_variation_axes`, which takes the
        // mode flag as a parameter so the release arm is testable; the ONLY
        // production call site passes `DEBUG_MODE`, the direct port of
        // `comptime builtin.mode == .Debug`. Same shape as `face/coretext.rs`.
        if let Err(err) = log_axes(&result) {
            // The reference's errdefers, in their reverse-declaration order:
            // `lib.alloc.destroy(ft_mutex)` then `hb_font.destroy()`. The FT face
            // itself belongs to the caller (`initFile`/`init` hold
            // `errdefer face.deinit()`), so it is NOT freed here, and `Face` has
            // no `Drop` that could free either handle a second time.
            // SAFETY: `ft_mutex` came from `Box::into_raw` above and nothing else
            // owns it; `result` is dropped without running any handle teardown.
            unsafe { drop(Box::from_raw(result.ft_mutex)) };
            result.hb_font.destroy();
            return Err(err);
        }

        Ok(result)
    }

    // PORT NOTE: `deinit`.
    pub fn deinit(&mut self) {
        #[cfg(test)]
        crate::face::test_probe::note_face_deinit();
        // self.lib.alloc.destroy(self.ft_mutex);
        unsafe {
            drop(Box::from_raw(self.ft_mutex));
        }
        {
            let _guard = unsafe { (*self.lib.mutex).lock().unwrap() };
            self.face.deinit();
        }
        self.hb_font.destroy();
        // self.* = undefined;
        // PORT NOTE: no Rust equivalent for `self.* = undefined`; the caller's
        // storage is left as-is (and dropped normally).
    }

    /// Returns the font name. If allocation is required, buf will be used,
    /// but sometimes allocation isn't required and a static string is
    /// returned.
    // PORT NOTE: the reference returns `[]const u8` that points at `buf` (the
    // decoded UTF-16 name), at FreeType-owned memory (the raw name record), or
    // at a `""` literal; so does the port, the result borrowing both `self`
    // (whose `FT_Face` owns the record) and `buf`. CORRECTION (task T-F,
    // S91-F9): the port used to copy the raw record into `buf`, truncating a
    // name longer than `buf`.
    pub fn name<'a>(&'a self, buf: &'a mut [u8]) -> Result<&'a [u8], Error> {
        let count = self.face.get_sfnt_name_count();

        // We look for the font family entry.
        for i in 0..count {
            let entry = match self.face.get_sfnt_name(i) {
                Ok(e) => e,
                Err(_) => continue,
            };
            if entry.name_id == c::TT_NAME_ID_FONT_FAMILY as c::FT_UShort {
                let string =
                    unsafe { std::slice::from_raw_parts(entry.string, entry.string_len as usize) };
                // There are other encodings that are something other than UTF-8
                // but this is one we've seen "in the wild" so far.
                if entry.platform_id == c::TT_PLATFORM_MICROSOFT as c::FT_UShort
                    && entry.encoding_id == c::TT_MS_ID_UNICODE_CS as c::FT_UShort
                {
                    'skip: {
                        if string.len() % 2 != 0 {
                            break 'skip;
                        }
                        if string.len() > 1024 {
                            break 'skip;
                        }
                        let mut tmp = [0u16; 512];
                        let max = string.len() / 2;
                        // for (@as([]const u16, @ptrCast(@alignCast(string))), 0..)
                        //     |c, j| tmp[j] = @byteSwap(c);
                        // PORT NOTE: the reference reinterprets the byte slice as a
                        // native-endian `[]u16` then byteswaps (correct on the
                        // little-endian targets rustty/ghostty build for); this
                        // reproduces exactly that (native read + swap).
                        for j in 0..max {
                            tmp[j] =
                                u16::from_ne_bytes([string[2 * j], string[2 * j + 1]]).swap_bytes();
                        }
                        // const len = std.unicode.utf16LeToUtf8(buf, tmp[0..max]) catch return string;
                        //
                        // PORT NOTE: reference defect (R13), UPSTREAM-DEFECTS.md UD190.
                        // `utf16LeToUtf8` does not check that `buf` can hold the result. When the
                        // UTF-8 form of a family record (at most 512 UTF-16 units) does not fit,
                        // Debug and ReleaseSafe panic, at `utf8Encode`'s assertion ("reached
                        // unreachable code") or, for a long enough ASCII run, at the bounds check
                        // of the vectorized ASCII fast path ("index out of bounds"); ReleaseFast
                        // writes past `buf` and returns a slice past its end, which is undefined
                        // behaviour. Callers pass 1024 bytes, or 256 (Collection's synthetic
                        // bold/italic log, and the `DeferredFace.name` callers on
                        // `freetype_windows`). Measured (Zig 0.16.0, aarch64-macos, a copy of
                        // `freetype.zig:170-178`): a 256-byte buffer overflows from 86 CJK
                        // characters (U+6C34), 129 two-byte characters (U+00E9) or 257 ASCII
                        // characters; a 1024-byte buffer from 342 CJK characters. The name comes
                        // from a font file. The port takes the reference's own fallback for a name
                        // it cannot decode, `catch return string`: the raw record.
                        let len = match utf16_le_to_utf8(buf, &tmp[..max]) {
                            Some(len) => len,
                            None => return Ok(string),
                        };
                        return Ok(&buf[..len]);
                    }
                }
                // return string;
                return Ok(string);
            }
        }

        Ok(b"")
    }

    /// Return a new face that is the same as this but also has synthetic
    /// bold applied.
    // PORT NOTE: `syntheticBold`.
    pub fn synthetic_bold(&self, opts: Options) -> Result<Face, Error> {
        // Increase face ref count
        self.face.ref_();
        // errdefer self.face.deinit();

        match Face::init_face(self.lib, self.face, opts) {
            Ok(mut f) => {
                f.synthetic = self.synthetic;
                f.synthetic.bold = true;
                Ok(f)
            }
            Err(e) => {
                self.face.deinit();
                Err(e)
            }
        }
    }

    /// Return a new face that is the same as this but has a transformation
    /// matrix applied to italicize it.
    // PORT NOTE: `syntheticItalic`.
    pub fn synthetic_italic(&self, opts: Options) -> Result<Face, Error> {
        // Increase face ref count
        self.face.ref_();
        // errdefer self.face.deinit();

        match Face::init_face(self.lib, self.face, opts) {
            Ok(mut f) => {
                f.synthetic = self.synthetic;
                f.synthetic.italic = true;
                Ok(f)
            }
            Err(e) => {
                self.face.deinit();
                Err(e)
            }
        }
    }

    /// Resize the font in-place. If this succeeds, the caller is responsible
    /// for clearing any glyph caches, font atlas data, etc.
    // PORT NOTE: `setSize`.
    pub fn set_size(&mut self, opts: Options) -> Result<(), Error> {
        // Test-only fail-point (W9F-02): lets unit tests force the fallible
        // `setSize` arm so its error-path cleanup can be proven.
        #[cfg(test)]
        if crate::face::test_probe::set_size_failpoint_should_fail() {
            return Err(Error::OutOfMemory);
        }

        set_size_(self.face, opts.size)?;
        self.size = opts.size;
        Ok(())
    }

    /// The reference's `if (comptime builtin.mode == .Debug) mm: { ... }`
    /// variation-axes dump from `initFace` (`face/freetype.zig:116-135`),
    /// extracted into a method so both arms are independently testable.
    /// `debug_mode` is the port of `comptime builtin.mode == .Debug`; the sole
    /// production call site passes [`DEBUG_MODE`].
    ///
    /// Purely diagnostic: it emits `log.debug` records and has no effect on the
    /// face. The reference's `try result.name(&buf)` and `try face.getMMVar()`
    /// PROPAGATE, so this returns `Result` and `init_face` uses `?` on it — the
    /// error paths are part of the ported behaviour, not just the emission.
    fn log_variation_axes(&self, debug_mode: bool) -> Result<(), Error> {
        if !debug_mode {
            return Ok(());
        }

        // if (!face.hasMultipleMasters()) break :mm;
        if !self.face.has_multiple_masters() {
            return Ok(());
        }

        // var buf: [1024]u8 = undefined;
        // log.debug("variation axes font={s}", .{try result.name(&buf)});
        //
        // PORT NOTE: `{s}` renders the raw name bytes; the lossy UTF-8
        // conversion is confined to the diagnostic, not to any data path.
        let mut buf = [0u8; 1024];
        log::debug!(
            target: LOG_SCOPE,
            "variation axes font={}",
            String::from_utf8_lossy(self.name(&mut buf)?)
        );

        // const mm = try face.getMMVar();
        // defer lib.lib.doneMMVar(mm);
        let mm = self.face.get_mm_var()?;

        // for (0..mm.num_axis) |i| { ... }
        for i in 0..unsafe { (*mm).num_axis } as usize {
            // const axis = mm.axis[i];
            let axis = unsafe { *(*mm).axis.add(i) };

            // const id_raw = std.math.cast(c_int, axis.tag) orelse continue;
            let id_raw: core::ffi::c_int = match core::ffi::c_int::try_from(axis.tag) {
                Ok(v) => v,
                Err(_) => continue,
            };

            // const id: font.face.Variation.Id = @bitCast(id_raw);
            //
            // PORT NOTE: `Variation.Id` is `packed struct(u32) { d, c, b, a }`,
            // so the FIRST field takes the least-significant byte — the same
            // layout `Id::to_u32` documents, read in the opposite direction.
            let bits = id_raw as u32;
            let id = super::Id {
                d: (bits & 0xff) as u8,
                c: ((bits >> 8) & 0xff) as u8,
                b: ((bits >> 16) & 0xff) as u8,
                a: ((bits >> 24) & 0xff) as u8,
            };

            // log.debug("variation axis: name={s} id={s} min={} max={} def={}", .{
            //     std.mem.sliceTo(axis.name, 0), id.str(),
            //     axis.minimum >> 16, axis.maximum >> 16, axis.def >> 16,
            // });
            //
            // PORT NOTE: `axis.name` is a NUL-terminated C string owned by
            // FreeType; `sliceTo(.., 0)` is `CStr::from_ptr`. A NULL `name` is
            // not something `sliceTo` would tolerate, but FreeType can leave it
            // unset, so the port renders `""` there rather than dereferencing
            // NULL — a diagnostic must not be able to crash the process.
            let name: &[u8] = if axis.name.is_null() {
                b""
            } else {
                unsafe { core::ffi::CStr::from_ptr(axis.name) }.to_bytes()
            };
            log::debug!(
                target: LOG_SCOPE,
                "variation axis: name={} id={} min={} max={} def={}",
                String::from_utf8_lossy(name),
                String::from_utf8_lossy(&id.str()),
                axis.minimum >> 16,
                axis.maximum >> 16,
                axis.def >> 16
            );
        }

        self.lib.lib.done_mm_var(mm);
        Ok(())
    }

    /// Set the variation axes for this font. This will modify this font
    /// in-place.
    // PORT NOTE: `setVariations`.
    pub fn set_variations(&mut self, vs: &[super::Variation], opts: Options) -> Result<(), Error> {
        let _ = opts;

        // If this font doesn't support variations, we can't do anything.
        if !self.face.has_multiple_masters() || vs.is_empty() {
            return Ok(());
        }

        // Freetype requires that we send ALL coordinates in at once so the
        // first thing we have to do is get all the vars and put them into
        // an array.
        let mm = self.face.get_mm_var()?;
        // defer self.lib.lib.doneMMVar(mm);
        let result = (|| -> Result<(), Error> {
            // To avoid allocations, we cap the number of variation axes we can
            // support. This is arbitrary but Firefox caps this at 16 so I
            // feel like that's probably safe... and we do double cause its
            // cheap.
            let mut coords_buf: [c::FT_Fixed; 32] = [0; 32];
            let n = std::cmp::min(coords_buf.len(), unsafe { (*mm).num_axis } as usize);
            let coords = &mut coords_buf[..n];
            self.face.get_var_design_coordinates(coords)?;

            // Now we go through each axis and see if its set. This is slow
            // but there usually aren't many axes and usually not many set
            // variations, either.
            // PORT NOTE: `for (0..mm.num_axis) |i|` — the loop indexes both the
            // FFI `axis` pointer and `coords[i]`, so the index form is faithful.
            #[allow(clippy::needless_range_loop)]
            for i in 0..unsafe { (*mm).num_axis } as usize {
                let axis = unsafe { *(*mm).axis.add(i) };
                // const id = std.math.cast(u32, axis.tag) orelse continue;
                let id: u32 = match u32::try_from(axis.tag) {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                for v in vs {
                    if id == v.id.to_u32() {
                        // PORT NOTE: reference defect (R13), UPSTREAM-DEFECTS.md
                        // UD98. `font-variation = wght=inf` (or any value whose
                        // 16.16 form leaves `FT_Fixed`, e.g. `1e20`) parses, and
                        // the reference's `@intFromFloat(v.value * 65536)` traps
                        // in ReleaseSafe (measured, Zig 0.16.0; ReleaseFast
                        // gives the `c_long` max, NaN gives 0 in every mode).
                        // Rust's `as` saturates to those same ReleaseFast values;
                        // FreeType keeps the design coordinate as given and the
                        // face still renders (task R13-D's test).
                        coords[i] = (v.value * 65536.0) as c::FT_Fixed;
                        break;
                    }
                }
            }

            // Set them!
            self.face.set_var_design_coordinates(coords)?;
            Ok(())
        })();
        self.lib.lib.done_mm_var(mm);
        result
    }

    /// Returns the glyph index for the given Unicode code point. If this
    /// face doesn't support this glyph, null is returned.
    // PORT NOTE: `glyphIndex`.
    pub fn glyph_index(&self, cp: u32) -> Option<u32> {
        self.face.get_char_index(cp)
    }

    /// Returns true if this font is colored. This can be used by callers to
    /// determine what kind of atlas to pass in.
    // PORT NOTE: `hasColor`.
    pub fn has_color(&self) -> bool {
        self.face.has_color()
    }

    /// Returns true if the given glyph ID is colorized.
    // PORT NOTE: `isColorGlyph`.
    pub fn is_color_glyph(&self, glyph_id: u32) -> bool {
        let _guard = unsafe { (*self.ft_mutex).lock().unwrap() };

        // Load the glyph and see what pixel mode it renders with.
        // All modes other than BGRA are non-color.
        // If the glyph fails to load, just return false.
        if self
            .face
            .load_glyph(
                glyph_id,
                LoadFlags {
                    render: true,
                    color: self.face.has_color(),
                    // NO_SVG set to true because we don't currently support
                    // rendering SVG glyphs under FreeType, since that requires
                    // bundling another dependency to handle rendering the SVG.
                    no_svg: true,
                    ..Default::default()
                },
            )
            .is_err()
        {
            return false;
        }

        let glyph = unsafe { (*self.face.handle).glyph };

        unsafe { (*glyph).bitmap.pixel_mode == c::FT_Pixel_Mode__FT_PIXEL_MODE_BGRA as u8 }
    }

    /// Set the load flags to use when loading a glyph for measurement or
    /// rendering.
    // PORT NOTE: `glyphLoadFlags`.
    fn glyph_load_flags(&self, constrained: bool) -> LoadFlags {
        // Hinting should only be enabled if the configured load flags specify
        // it and the provided constraint doesn't actually do anything, since
        // if it does, then it'll mess up the hinting anyway when it moves or
        // resizes the glyph.
        let do_hinting = self.load_flags.hinting && !constrained;

        LoadFlags {
            // If our glyph has color, we want to render the color
            color: self.face.has_color(),

            // We don't render, because we'll invoke the render
            // manually after applying constraints further down.
            render: false,

            // use options from config
            no_hinting: !do_hinting,
            force_autohint: self.load_flags.force_autohint,
            no_autohint: !self.load_flags.autohint,

            // If we're gonna be rendering this glyph in monochrome,
            // then we should use the monochrome hinter as well, or
            // else it won't look very good at all.
            //
            // Otherwise if the user asked for light hinting we
            // use that, otherwise we just use the normal target.
            target: if self.load_flags.monochrome {
                Target::Mono
            } else if self.load_flags.light {
                Target::Light
            } else {
                Target::Normal
            },

            // NO_SVG set to true because we don't currently support rendering
            // SVG glyphs under FreeType, since that requires bundling another
            // dependency to handle rendering the SVG.
            no_svg: true,

            ..Default::default()
        }
    }

    /// Render a glyph using the glyph index. The rendered glyph is stored in the
    /// given texture atlas.
    // PORT NOTE: `renderGlyph`.
    pub fn render_glyph<A: Allocator + Copy>(
        &self,
        alloc: A,
        atlas: &mut Atlas<A>,
        glyph_index: u32,
        opts: RenderOptions,
    ) -> Result<Glyph, Error> {
        let _guard = unsafe { (*self.ft_mutex).lock().unwrap() };

        // Load the glyph.
        self.face.load_glyph(
            glyph_index,
            self.glyph_load_flags(opts.constraint.does_anything()),
        )?;
        let glyph = unsafe { (*self.face.handle).glyph };

        // For synthetic bold, we embolden the glyph.
        if self.synthetic.bold {
            // We need to scale the embolden amount based on the font size.
            // This is a heuristic I found worked well across a variety of
            // founts: 1 pixel per 64 units of height.
            let font_height: f64 = unsafe { (*(*self.face.handle).size).metrics.height } as f64;
            let ratio: f64 = 64.0 / 2048.0;
            let amount = (font_height * ratio).ceil();
            unsafe {
                c::FT_Outline_Embolden(&mut (*glyph).outline, amount as c::FT_Pos);
            }
        }

        // We get a rect that represents the position
        // and size of the glyph before constraints.
        let rect = get_glyph_size(glyph);

        // If our glyph is smaller than a quarter pixel in either axis
        // then it has no outlines or they're too small to render.
        //
        // In this case we just return 0-sized glyph struct.
        if rect.width < 0.25 || rect.height < 0.25 {
            return Ok(Glyph {
                width: 0,
                height: 0,
                offset_x: 0,
                offset_y: 0,
                atlas_x: 0,
                atlas_y: 0,
            });
        }

        let metrics = opts.grid_metrics;
        let cell_width: f64 = metrics.cell_width as f64;
        let cell_height: f64 = metrics.cell_height as f64;

        // Next we apply any constraints to get the final size of the glyph.
        let constraint = opts.constraint;

        // We need to add the baseline position before passing to the constrain
        // function since it operates on cell-relative positions, not baseline.
        let cell_baseline: f64 = metrics.cell_baseline as f64;

        let glyph_size = constraint.constrain(
            Size {
                width: rect.width,
                height: rect.height,
                x: rect.x,
                y: rect.y + cell_baseline,
            },
            metrics,
            opts.constraint_width,
        );

        let mut width = glyph_size.width;
        let mut height = glyph_size.height;
        let mut x = glyph_size.x;
        let mut y = glyph_size.y;

        // We center all glyphs within the pixel-rounded and adjusted
        // cell width if it's larger than the face width, so that they
        // aren't weirdly off to the left.
        //
        // We don't do this if the glyph has a stretch constraint,
        // since in that case the position was already calculated with the
        // new cell width in mind.
        if (constraint.size != ConstraintSize::Stretch) && (metrics.face_width < cell_width) {
            // We add half the difference to re-center.
            //
            // NOTE: We round this to a whole-pixel amount because under
            //       FreeType, the outlines will be hinted, which isn't
            //       the case under CoreText. If we move the outlines by
            //       a non-whole-pixel amount, it completely ruins the
            //       hinting.
            x += ((cell_width - metrics.face_width) / 2.0).round();
        }

        // If this is a bitmap glyph, it will always render as full pixels,
        // not fractional pixels, so we need to quantize its position and
        // size accordingly to align to full pixels so we get good results.
        if unsafe { (*glyph).format }
            == c::FT_Glyph_Format__FT_GLYPH_FORMAT_BITMAP as c::FT_Glyph_Format
        {
            width = cell_width - (cell_width - width - x).round() - x.round();
            height = cell_height - (cell_height - height - y).round() - y.round();
            x = x.round();
            y = y.round();
        }

        // Now we can render the glyph.
        let mut bitmap: c::FT_Bitmap = unsafe { std::mem::zeroed() };
        unsafe {
            c::FT_Bitmap_Init(&mut bitmap);
        }
        // defer _ = freetype.c.FT_Bitmap_Done(self.lib.lib.handle, &bitmap);
        let _bitmap_guard = BitmapGuard {
            lib: self.lib.lib.handle,
            bitmap: &mut bitmap,
        };

        let glyph_format = unsafe { (*glyph).format };
        if glyph_format == c::FT_Glyph_Format__FT_GLYPH_FORMAT_OUTLINE as c::FT_Glyph_Format {
            // Manually adjust the glyph outline with this transform.
            //
            // This offers better precision than using the freetype transform
            // matrix, since that has 16.16 coefficients, and also I was having
            // weird issues that I can only assume where due to freetype doing
            // some bad caching or something when I did this using the matrix.
            let scale_x = width / rect.width;
            let scale_y = height / rect.height;
            let skew: f64 = if self.synthetic.italic {
                // We skew by 12 degrees to synthesize italics.
                (12.0_f64).to_radians().tan()
            } else {
                0.0
            };

            let outline = unsafe { &mut (*glyph).outline };
            let points = unsafe {
                std::slice::from_raw_parts_mut(outline.points, outline.n_points as usize)
            };
            for p in points.iter_mut() {
                // Convert to f64 for processing
                let mut px = f26dot6_to_f64(p.x);
                let mut py = f26dot6_to_f64(p.y);

                // Subtract original bearings
                px -= rect.x;
                py -= rect.y;

                // Scale
                px *= scale_x;
                py *= scale_y;

                // Add new bearings
                px += x;
                py += y - cell_baseline;

                // Skew
                px += py * skew;

                // Convert back and store
                p.x = F26Dot6::from(px).to_raw() as c::FT_Pos;
                p.y = F26Dot6::from(py).to_raw() as c::FT_Pos;
            }

            self.face.render_glyph(if self.load_flags.monochrome {
                RenderMode::Mono
            } else {
                RenderMode::Normal
            })?;

            // Copy the glyph's bitmap, making sure
            // that it's 8bpp and densely packed.
            if unsafe {
                c::FT_Bitmap_Convert(
                    self.lib.lib.handle,
                    std::ptr::addr_of!((*glyph).bitmap),
                    &mut bitmap,
                    1,
                )
            } != 0
            {
                return Err(Error::BitmapHandlingError);
            }
        } else if glyph_format == c::FT_Glyph_Format__FT_GLYPH_FORMAT_BITMAP as c::FT_Glyph_Format {
            // If our glyph has a non-color bitmap, we need
            // to convert it to dense 8bpp so that the scale
            // operation works correctly.
            let pixel_mode = unsafe { (*glyph).bitmap.pixel_mode };
            if pixel_mode == c::FT_Pixel_Mode__FT_PIXEL_MODE_BGRA as u8
                || pixel_mode == c::FT_Pixel_Mode__FT_PIXEL_MODE_GRAY as u8
            {
                // no conversion needed
            } else {
                // Make sure the slot owns its bitmap,
                // since we'll be modifying it here.
                if unsafe { c::FT_GlyphSlot_Own_Bitmap(glyph) } != 0 {
                    return Err(Error::BitmapHandlingError);
                }

                let mut converted: c::FT_Bitmap = unsafe { std::mem::zeroed() };
                unsafe {
                    c::FT_Bitmap_Init(&mut converted);
                }
                if unsafe {
                    c::FT_Bitmap_Convert(
                        self.lib.lib.handle,
                        std::ptr::addr_of!((*glyph).bitmap),
                        &mut converted,
                        1,
                    )
                } != 0
                {
                    return Err(Error::BitmapHandlingError);
                }
                // Free the existing glyph bitmap and
                // replace it with the converted one.
                unsafe {
                    c::FT_Bitmap_Done(self.lib.lib.handle, &mut (*glyph).bitmap);
                    (*glyph).bitmap = converted;
                }
            }

            let glyph_bitmap = unsafe { (*glyph).bitmap };

            // Round our target width and height
            // as the size for our scaled bitmap.
            let w: u32 = width.round() as u32;
            let h: u32 = height.round() as u32;
            let pitch = w * (atlas.format().depth() as u32);

            // Allocate a buffer for our scaled bitmap.
            //
            // We'll copy this to the original bitmap once we're
            // done so we can free it at the end of this scope.
            let n = (pitch as usize) * (h as usize);
            let mut buf = allocator_api2::vec::Vec::<u8, A>::with_capacity_in(n, alloc);
            buf.resize(n, 0);
            // defer alloc.free(buf); — `buf` drops at end of scope.

            // Resize
            if unsafe {
                crate::stb::stbir_resize_uint8(
                    glyph_bitmap.buffer,
                    glyph_bitmap.width as c_int,
                    glyph_bitmap.rows as c_int,
                    glyph_bitmap.pitch,
                    buf.as_mut_ptr(),
                    w as c_int,
                    h as c_int,
                    pitch as c_int,
                    atlas.format().depth() as c_int,
                )
            } == 0
            {
                // This should never fail because this is a
                // fairly straightforward in-memory operation...
                return Err(Error::GlyphResizeFailed);
            }

            let mut scaled_bitmap: c::FT_Bitmap = unsafe { std::mem::zeroed() };
            scaled_bitmap.buffer = buf.as_mut_ptr();
            scaled_bitmap.width = w;
            scaled_bitmap.rows = h;
            scaled_bitmap.pitch = pitch as c_int;
            scaled_bitmap.pixel_mode = glyph_bitmap.pixel_mode;
            scaled_bitmap.num_grays = glyph_bitmap.num_grays;

            // Replace the bitmap's buffer and size info.
            if unsafe { c::FT_Bitmap_Copy(self.lib.lib.handle, &scaled_bitmap, &mut bitmap) } != 0 {
                return Err(Error::BitmapHandlingError);
            }

            // Update the bearings to account for the new positioning.
            unsafe {
                (*glyph).bitmap_top = (y - cell_baseline + height).floor() as c::FT_Int;
                (*glyph).bitmap_left = x.floor() as c::FT_Int;
            }
        } else {
            // Glyph formats are tags, so we can
            // output a semi-readable error here.
            // Use @bitCast to u32 because MSVC translates C enums
            // as signed int, while GCC/Clang uses unsigned int.
            //
            // const tag: u32 = @bitCast(f);
            // log.err(
            //     "Can't render glyph with unsupported glyph format \"{s}\"",
            //     .{[4]u8{
            //         @truncate(tag >> 24),
            //         @truncate(tag >> 16),
            //         @truncate(tag >> 8),
            //         @truncate(tag >> 0),
            //     }},
            // );
            //
            // PORT NOTE: `@bitCast(f)` reinterprets the glyph-format enum's bits
            // as `u32` — the reference's comment above explains why the cast is
            // through `u32` and not the C signed int. The four `@truncate`s spell
            // the tag's four bytes MOST-significant first; `{s}` then renders
            // them as text.
            let tag: u32 = glyph_format as u32;
            log::error!(
                target: LOG_SCOPE,
                "Can't render glyph with unsupported glyph format \"{}\"",
                String::from_utf8_lossy(&[
                    (tag >> 24) as u8,
                    (tag >> 16) as u8,
                    (tag >> 8) as u8,
                    tag as u8,
                ])
            );
            return Err(Error::UnsupportedGlyphFormat);
        }

        // If this is a color glyph but we're trying to render it to the
        // grayscale atlas, or vice versa, then we throw and error. Maybe
        // in the future we could convert, but for now it should be fine.
        if bitmap.pixel_mode == c::FT_Pixel_Mode__FT_PIXEL_MODE_GRAY as u8 {
            if atlas.format() != Format::Grayscale {
                return Err(Error::WrongAtlas);
            }
        } else if bitmap.pixel_mode == c::FT_Pixel_Mode__FT_PIXEL_MODE_BGRA as u8 {
            if atlas.format() != Format::Bgra {
                return Err(Error::WrongAtlas);
            }
        } else {
            // log.warn("glyph={} pixel mode={}", .{ glyph_index, bitmap.pixel_mode });
            //
            // PORT NOTE: this warn is emitted IMMEDIATELY BEFORE the `@panic`,
            // and that ordering is the whole point of it — it is the only record
            // of which glyph and which pixel mode caused the abort. It must stay
            // above the `panic!`.
            log::warn!(
                target: LOG_SCOPE,
                "glyph={} pixel mode={}",
                glyph_index,
                bitmap.pixel_mode
            );
            panic!("unsupported pixel mode");
        }

        // Our whole-pixel bearings for the final glyph.
        // The fractional portion will be included in the rasterized position.
        //
        // For the Y position, FreeType's `bitmap_top` is the distance from the
        // baseline to the top of the glyph, but we need the distance from the
        // bottom of the cell to the bottom of the glyph, so first we add the
        // baseline to get the distance from the bottom of the cell to the top
        // of the glyph, then we subtract the height of the glyph to get the
        // bottom.
        let px_x: i32 = unsafe { (*glyph).bitmap_left };
        let px_y: i32 =
            unsafe { (*glyph).bitmap_top } + (metrics.cell_baseline as i32) - (bitmap.rows as i32);

        let px_width = bitmap.width;
        let px_height = bitmap.rows;
        let len: usize = (bitmap.pitch.unsigned_abs() as usize) * (bitmap.rows as usize);

        // If our bitmap is grayscale, make sure to multiply all pixel
        // values by the right factor to bring `num_grays` up to 256.
        //
        // This is necessary because FT_Bitmap_Convert doesn't do this,
        // it just sets num_grays to the correct number and uses the
        // original smaller pixel values.
        if bitmap.pixel_mode == c::FT_Pixel_Mode__FT_PIXEL_MODE_GRAY as u8 && bitmap.num_grays < 256
        {
            let factor: u8 = (255 / (bitmap.num_grays - 1)) as u8;
            let pixels = unsafe { std::slice::from_raw_parts_mut(bitmap.buffer, len) };
            for p in pixels.iter_mut() {
                *p *= factor;
            }
            bitmap.num_grays = 256;
        }

        // Must have non-empty bitmap because we return earlier if zero.
        // We assume the rest of this that it is non-zero so this is important.
        assert(px_width > 0 && px_height > 0);

        // If this doesn't match then something is wrong.
        assert(px_width * (atlas.format().depth() as u32) == bitmap.pitch as u32);

        // Allocate our texture atlas region and copy our bitmap in to it.
        let region = atlas.reserve(alloc, px_width, px_height)?;
        atlas.set(region, unsafe {
            std::slice::from_raw_parts(bitmap.buffer, len)
        });

        // This should be the distance from the bottom of
        // the cell to the top of the glyph's bounding box.
        let offset_y: i32 = px_y + (px_height as i32);

        // This should be the distance from the left of
        // the cell to the left of the glyph's bounding box.
        let offset_x: i32 = px_x;

        Ok(Glyph {
            width: px_width,
            height: px_height,
            offset_x,
            offset_y,
            atlas_x: region.x,
            atlas_y: region.y,
        })
    }

    /// Convert 16.6 pixel format to pixels based on the scale factor of the
    /// current font size.
    // PORT NOTE: `unitsToPxY`.
    // PORT NOTE: kept verbatim though currently unused in the reference too
    // (a private helper); `#[allow(dead_code)]` mirrors Zig not warning on it.
    #[allow(dead_code)]
    fn units_to_px_y(&self, units: i32) -> i32 {
        ft::mul_fix(
            units,
            unsafe { (*(*self.face.handle).size).metrics.y_scale } as i32,
        ) >> 6
    }

    /// Get the `FaceMetrics` for this face.
    // PORT NOTE: `getMetrics`.
    pub fn get_metrics(&self) -> FaceMetrics {
        let face = self.face;

        let size_metrics = unsafe { (*(*face.handle).size).metrics };

        // This code relies on this assumption, and it should always be
        // true since we don't do any non-uniform scaling on the font ever.
        assert(size_metrics.x_ppem == size_metrics.y_ppem);

        // Read the 'head' table out of the font data.
        let head_ = face.get_sfnt_head();

        // Read the 'post' table out of the font data.
        let post_ = face.get_sfnt_post();

        // Read the 'OS/2' table out of the font data.
        let os2_: Option<*mut c::TT_OS2> = face
            .get_sfnt_os2()
            .filter(|&os2| unsafe { (*os2).version } != 0xFFFF);

        // Read the 'hhea' table out of the font data.
        let hhea_ = face.get_sfnt_hhea();

        // Whether the font is in a scalable format. We need to know this
        // because many of the metrics provided by FreeType are invalid for
        // non-scalable fonts.
        let is_scalable =
            unsafe { (*face.handle).face_flags } & (c::FT_FACE_FLAG_SCALABLE as c::FT_Long) != 0;

        // We get the UPM from the head table.
        //
        // If we have no head, but it is a scalable face, take the UPM from
        // FreeType's units_per_EM, otherwise we'll assume that UPM == PPEM.
        let units_per_em: c::FT_UShort = if let Some(head) = head_ {
            unsafe { (*head).Units_Per_EM }
        } else if is_scalable {
            unsafe { (*face.handle).units_per_EM }
        } else {
            size_metrics.y_ppem
        };
        let px_per_em: f64 = size_metrics.y_ppem as f64;
        let px_per_unit = px_per_em / (units_per_em as f64);

        let (ascent, descent, line_gap): (f64, f64, f64) = 'vertical_metrics: {
            let hhea = match hhea_ {
                Some(h) => h,
                None => {
                    // If we couldn't get the hhea table, rely on metrics from FreeType.
                    let ascender = f26dot6_to_f64(size_metrics.ascender);
                    let descender = f26dot6_to_f64(size_metrics.descender);
                    let height = f26dot6_to_f64(size_metrics.height);
                    break 'vertical_metrics (
                        ascender,
                        descender,
                        // We compute the line gap by adding the (negative) descender
                        // and subtracting the (positive) ascender from the line height
                        // to get the remaining gap size.
                        //
                        // NOTE: This might always be 0... but it doesn't hurt to do.
                        height + descender - ascender,
                    );
                }
            };

            let hhea_ascent: f64 = f64::from(unsafe { (*hhea).Ascender });
            let hhea_descent: f64 = f64::from(unsafe { (*hhea).Descender });
            let hhea_line_gap: f64 = f64::from(unsafe { (*hhea).Line_Gap });

            // If our font has no OS/2 table, then we just
            // blindly use the metrics from the hhea table.
            let os2 = match os2_ {
                Some(o) => o,
                None => {
                    break 'vertical_metrics (
                        hhea_ascent * px_per_unit,
                        hhea_descent * px_per_unit,
                        hhea_line_gap * px_per_unit,
                    )
                }
            };

            let os2_ascent: f64 = f64::from(unsafe { (*os2).sTypoAscender });
            let os2_descent: f64 = f64::from(unsafe { (*os2).sTypoDescender });
            let os2_line_gap: f64 = f64::from(unsafe { (*os2).sTypoLineGap });

            // If the font says to use typo metrics, trust it.
            // (The USE_TYPO_METRICS bit is bit 7)
            if unsafe { (*os2).fsSelection } & (1 << 7) != 0 {
                break 'vertical_metrics (
                    os2_ascent * px_per_unit,
                    os2_descent * px_per_unit,
                    os2_line_gap * px_per_unit,
                );
            }

            // Otherwise we prefer the height metrics from 'hhea' if they
            // are available, or else OS/2 sTypo* metrics, and if all else
            // fails then we use OS/2 usWin* metrics.
            //
            // This is not "standard" behavior, but it's our best bet to
            // account for fonts being... just weird. It's pretty much what
            // FreeType does to get its generic ascent and descent metrics.

            if unsafe { (*hhea).Ascender } != 0 || unsafe { (*hhea).Descender } != 0 {
                break 'vertical_metrics (
                    hhea_ascent * px_per_unit,
                    hhea_descent * px_per_unit,
                    hhea_line_gap * px_per_unit,
                );
            }

            if os2_ascent != 0.0 || os2_descent != 0.0 {
                break 'vertical_metrics (
                    os2_ascent * px_per_unit,
                    os2_descent * px_per_unit,
                    os2_line_gap * px_per_unit,
                );
            }

            let win_ascent: f64 = f64::from(unsafe { (*os2).usWinAscent });
            let win_descent: f64 = f64::from(unsafe { (*os2).usWinDescent });
            break 'vertical_metrics (
                win_ascent * px_per_unit,
                // usWinDescent is *positive* -> down unlike sTypoDescender
                // and hhea.Descender, so we flip its sign to fix this.
                -win_descent * px_per_unit,
                0.0,
            );
        };

        let (underline_position, underline_thickness): (Option<f64>, Option<f64>) = 'ul: {
            let post = match post_ {
                Some(p) => p,
                None => break 'ul (None, None),
            };

            // Some fonts have degenerate 'post' tables where the underline
            // thickness (and often position) are 0. We consider them null
            // if this is the case and use our own fallbacks when we calculate.
            let has_broken_underline = unsafe { (*post).underlineThickness } == 0;

            // If the underline position isn't 0 then we do use it,
            // even if the thickness is't properly specified.
            let pos: Option<f64> =
                if has_broken_underline && unsafe { (*post).underlinePosition } == 0 {
                    None
                } else {
                    Some(f64::from(unsafe { (*post).underlinePosition }) * px_per_unit)
                };

            let thick: Option<f64> = if has_broken_underline {
                None
            } else {
                Some(f64::from(unsafe { (*post).underlineThickness }) * px_per_unit)
            };

            (pos, thick)
        };

        // Similar logic to the underline above.
        let (strikethrough_position, strikethrough_thickness): (Option<f64>, Option<f64>) = 'st: {
            let os2 = match os2_ {
                Some(o) => o,
                None => break 'st (None, None),
            };

            let has_broken_strikethrough = unsafe { (*os2).yStrikeoutSize } == 0;

            let pos: Option<f64> =
                if has_broken_strikethrough && unsafe { (*os2).yStrikeoutPosition } == 0 {
                    None
                } else {
                    Some(f64::from(unsafe { (*os2).yStrikeoutPosition }) * px_per_unit)
                };

            let thick: Option<f64> = if has_broken_strikethrough {
                None
            } else {
                Some(f64::from(unsafe { (*os2).yStrikeoutSize }) * px_per_unit)
            };

            (pos, thick)
        };

        // Cell width is calculated by calculating the widest width of the
        // visible ASCII characters. Usually 'M' is widest but we just take
        // whatever is widest.
        //
        // ASCII height is calculated as the height of the overall bounding
        // box of the same characters.
        //
        // If we fail to load any visible ASCII we just use max_advance from
        // the metrics provided by FreeType, and set ascii_height to null as
        // it's optional.
        let (cell_width, ascii_height): (f64, Option<f64>) = {
            let _guard = unsafe { (*self.ft_mutex).lock().unwrap() };

            let mut max: f64 = 0.0;
            let mut top: f64 = 0.0;
            let mut bottom: f64 = 0.0;
            let mut ch: u8 = b' ';
            while ch < 127 {
                if let Some(glyph_index) = face.get_char_index(ch as u32) {
                    if face
                        .load_glyph(glyph_index, self.glyph_load_flags(false))
                        .is_ok()
                    {
                        let glyph = unsafe { (*face.handle).glyph };
                        max = max.max(f26dot6_to_f64(unsafe { (*glyph).advance.x }));
                        let rect = get_glyph_size(glyph);
                        top = top.max(rect.y + rect.height);
                        bottom = bottom.min(rect.y);
                    }
                }
                ch += 1;
            }

            // If we couldn't get valid measurements, just use
            // FreeType's max_advance and null, respectively.
            if max == 0.0 {
                max = f26dot6_to_f64(size_metrics.max_advance);
            }
            let rect_height: Option<f64> = {
                let estimate = top - bottom;
                if estimate <= 0.0 {
                    None
                } else {
                    Some(estimate)
                }
            };

            (max, rect_height)
        };

        // We use the cap and ex heights specified by the font if they're
        // available, otherwise we try to measure the `H` and `x` glyphs.
        let (cap_height, ex_height): (Option<f64>, Option<f64>) = 'heights: {
            if let Some(os2) = os2_ {
                // The OS/2 table does not include these metrics in version 1.
                if unsafe { (*os2).version } >= 2 {
                    break 'heights (
                        Some(f64::from(unsafe { (*os2).sCapHeight }) * px_per_unit),
                        Some(f64::from(unsafe { (*os2).sxHeight }) * px_per_unit),
                    );
                }
            }

            let cap: Option<f64> = {
                let _guard = unsafe { (*self.ft_mutex).lock().unwrap() };
                'cap: {
                    if let Some(glyph_index) = face.get_char_index('H' as u32) {
                        if face
                            .load_glyph(glyph_index, self.glyph_load_flags(false))
                            .is_ok()
                        {
                            break 'cap Some(
                                get_glyph_size(unsafe { (*face.handle).glyph }).height,
                            );
                        }
                    }
                    None
                }
            };
            let ex: Option<f64> = {
                let _guard = unsafe { (*self.ft_mutex).lock().unwrap() };
                'ex: {
                    if let Some(glyph_index) = face.get_char_index('x' as u32) {
                        if face
                            .load_glyph(glyph_index, self.glyph_load_flags(false))
                            .is_ok()
                        {
                            break 'ex Some(get_glyph_size(unsafe { (*face.handle).glyph }).height);
                        }
                    }
                    None
                }
            };

            (cap, ex)
        };

        // Measure "水" (CJK water ideograph, U+6C34) for our ic width.
        let ic_width: Option<f64> = {
            let _guard = unsafe { (*self.ft_mutex).lock().unwrap() };

            'ic_width: {
                let glyph = match face.get_char_index('水' as u32) {
                    Some(g) => g,
                    None => break 'ic_width None,
                };

                if face
                    .load_glyph(glyph, self.glyph_load_flags(false))
                    .is_err()
                {
                    break 'ic_width None;
                }

                let ft_glyph = unsafe { (*face.handle).glyph };

                // If the advance of the glyph is less than the width of the actual
                // glyph then we just treat it as invalid since it's probably wrong
                // and using it for size normalization will instead make the font
                // way too big.
                //
                // This can sometimes happen if there's a CJK font that has been
                // patched with the nerd fonts patcher and it butchers the advance
                // values so the advance ends up half the width of the actual glyph.
                let ft_glyph_width = get_glyph_size(ft_glyph).width;
                let advance = f26dot6_to_f64(unsafe { (*ft_glyph).advance.x });
                if ft_glyph_width > advance {
                    // var buf: [1024]u8 = undefined;
                    // const font_name = self.name(&buf) catch "<Error getting font name>";
                    // log.warn(
                    //     "(getMetrics) Width of glyph '水' for font \"{s}\" is greater than its advance ({d} > {d}), discarding ic_width metric.",
                    //     .{ font_name, ft_glyph_width, advance },
                    // );
                    //
                    // PORT NOTE: the reference's `catch "<Error getting font
                    // name>"` fallback is part of the message, not an error path;
                    // `{s}` renders the raw name bytes, so the lossy UTF-8
                    // conversion is confined to the diagnostic.
                    let mut buf = [0u8; 1024];
                    let font_name = self
                        .name(&mut buf)
                        .map(String::from_utf8_lossy)
                        .unwrap_or(std::borrow::Cow::Borrowed("<Error getting font name>"));
                    log::warn!(
                        target: LOG_SCOPE,
                        "(getMetrics) Width of glyph '水' for font \"{font_name}\" is greater than its advance ({ft_glyph_width} > {advance}), discarding ic_width metric."
                    );
                    break 'ic_width None;
                }

                Some(advance)
            }
        };

        FaceMetrics {
            px_per_em,

            cell_width,

            ascent,
            descent,
            line_gap,

            underline_position,
            underline_thickness,

            strikethrough_position,
            strikethrough_thickness,

            cap_height,
            ex_height,
            ascii_height,
            ic_width,
        }
    }

    /// Copy the font table data for the given tag.
    // PORT NOTE: `copyTable`.
    pub fn copy_table<A: Allocator + Copy>(
        &self,
        _alloc: A,
        tag: &[u8; 4],
    ) -> Result<Option<Vec<u8>>, Error> {
        // PORT NOTE: the reference threads `alloc` into `loadSfntTable`; the table
        // buffer here is a global-allocator `Vec<u8>` (the allocator choice is not
        // observable), so `_alloc` is accepted for call-shape fidelity only.
        Ok(self.face.load_sfnt_table(Tag::init(tag))?)
    }
}

/// Scope guard reproducing the reference's `defer FT_Bitmap_Done(lib, &bitmap)`.
struct BitmapGuard {
    lib: c::FT_Library,
    bitmap: *mut c::FT_Bitmap,
}

impl Drop for BitmapGuard {
    fn drop(&mut self) {
        unsafe {
            c::FT_Bitmap_Done(self.lib, self.bitmap);
        }
    }
}

/// If true, the default font features should be disabled for the given face.
// PORT NOTE: `quirks.disableDefaultFontFeatures`.
fn disable_default_font_features(face: &Face) -> bool {
    let _ = face;

    // This function used to do something, but we integrated the logic
    // we checked for directly into our shaping algorithm. It's likely
    // there are other broken fonts for other reasons so I'm keeping this
    // around so its easy to add more checks in the future.
    false
}

/// Get a rect that represents the position and size of the loaded glyph.
// PORT NOTE: `getGlyphSize`.
fn get_glyph_size(glyph: *mut c::FT_GlyphSlotRec_) -> Size {
    // If we're dealing with an outline glyph then we get the
    // outline's bounding box instead of using the built-in
    // metrics, since that's more precise and allows better
    // cell-fitting.
    if unsafe { (*glyph).format }
        == c::FT_Glyph_Format__FT_GLYPH_FORMAT_OUTLINE as c::FT_Glyph_Format
    {
        // Get the glyph's bounding box before we transform it at all.
        // We use this rather than the metrics, since it's more precise.
        let mut bbox: c::FT_BBox = unsafe { std::mem::zeroed() };
        unsafe {
            c::FT_Outline_Get_BBox(&mut (*glyph).outline, &mut bbox);
        }

        return Size {
            x: f26dot6_to_f64(bbox.xMin),
            y: f26dot6_to_f64(bbox.yMin),
            width: f26dot6_to_f64(bbox.xMax - bbox.xMin),
            height: f26dot6_to_f64(bbox.yMax - bbox.yMin),
        };
    }

    let m = unsafe { (*glyph).metrics };
    Size {
        x: f26dot6_to_f64(m.horiBearingX),
        y: f26dot6_to_f64(m.horiBearingY - m.height),
        width: f26dot6_to_f64(m.width),
        height: f26dot6_to_f64(m.height),
    }
}

/// Resize helper.
// PORT NOTE: `setSize_`.
fn set_size_(face: ft::Face, size: DesiredSize) -> Result<(), Error> {
    // If we have fixed sizes, we just have to try to pick the one closest
    // to what the user requested. Otherwise, we can choose an arbitrary
    // pixel size.
    if face.is_scalable() {
        let size_26dot6: i32 = (size.points * 64.0).round() as i32;
        face.set_char_size(0, size_26dot6, size.xdpi, size.ydpi)?;
        Ok(())
    } else {
        select_size_nearest(face, size.pixels().round() as u32)
    }
}

/// Selects the fixed size in the loaded face that is closest to the
/// requested pixel size.
// PORT NOTE: `selectSizeNearest`.
fn select_size_nearest(face: ft::Face, size: u32) -> Result<(), Error> {
    let mut i: i32 = 0;
    let mut best_i: i32 = 0;
    let mut best_diff: i32 = 0;
    while i < unsafe { (*face.handle).num_fixed_sizes } {
        let width = unsafe { (*(*face.handle).available_sizes.offset(i as isize)).width };
        let diff = (size as i32) - (width as i32);
        if i == 0 || diff < best_diff {
            best_diff = diff;
            best_i = i;
        }
        i += 1;
    }

    face.select_size(best_i)?;
    Ok(())
}

/// Convert 26.6 pixel format to f32
// PORT NOTE: `f26dot6ToFloat` (currently unused in the reference too).
#[allow(dead_code)]
fn f26dot6_to_float(v: c::FT_F26Dot6) -> f32 {
    (v >> 6) as f32
}

// PORT NOTE: `f26dot6ToF64`.
fn f26dot6_to_f64(v: c::FT_F26Dot6) -> f64 {
    F26Dot6::from_raw(v as i32).to_f64()
}

/// Decode UTF-16 code units into `buf` as UTF-8, returning the byte length.
// PORT NOTE: models `std.unicode.utf16LeToUtf8(buf, units)` — returns `None` on
// an invalid surrogate (the reference's errors, which `name` `catch`es and
// answers with the raw name string) or if the output does not fit. CORRECTION
// (task T-F): this said the reference `catch`es both; an output that does not
// fit is an assertion there, not an error (reference defect, UD190), and `name`
// answers it the same way as a decode error.
fn utf16_le_to_utf8(buf: &mut [u8], units: &[u16]) -> Option<usize> {
    let mut pos = 0usize;
    for r in char::decode_utf16(units.iter().copied()) {
        let ch = r.ok()?;
        let mut enc = [0u8; 4];
        let s = ch.encode_utf8(&mut enc);
        let sb = s.as_bytes();
        if pos + sb.len() > buf.len() {
            return None;
        }
        buf[pos..pos + sb.len()].copy_from_slice(sb);
        pos += sb.len();
    }
    Some(pos)
}

#[cfg(test)]
mod tests {
    use super::*;
    use allocator_api2::alloc::Global;
    use rustty_font::embedded;
    use rustty_font::metrics::Metrics;

    // PORT NOTE: the reference tests build `font.face.Options` with only `.size`
    // set (`freetype_load_flags` defaults). This helper mirrors that.
    fn opts(size: DesiredSize) -> Options {
        Options {
            size,
            freetype_load_flags: FreetypeLoadFlags::default(),
        }
    }

    fn size(points: f32, xdpi: u16, ydpi: u16) -> DesiredSize {
        DesiredSize { points, xdpi, ydpi }
    }

    /// JetBrains Mono with its `name` table replaced by one holding exactly
    /// `records` (`(platform, encoding, language, name id, bytes)`), in that
    /// order. The new table is appended and the directory entry repointed.
    fn font_with_names(records: &[(u16, u16, u16, u16, Vec<u8>)]) -> Vec<u8> {
        let mut table = Vec::new();
        let count = records.len() as u16;
        let string_offset = 6 + 12 * count;
        for v in [0u16, count, string_offset] {
            table.extend_from_slice(&v.to_be_bytes());
        }
        let mut storage = Vec::new();
        for (platform, encoding, language, name_id, bytes) in records {
            for v in [
                *platform,
                *encoding,
                *language,
                *name_id,
                bytes.len() as u16,
                storage.len() as u16,
            ] {
                table.extend_from_slice(&v.to_be_bytes());
            }
            storage.extend_from_slice(bytes);
        }
        table.extend_from_slice(&storage);

        let mut font = embedded::VARIABLE.to_vec();
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

    /// S91-F9: `Face.name` returns the FreeType-owned family record itself
    /// (`face/freetype.zig:161-181`, `return string`), so a raw name longer than
    /// the caller's buffer comes back whole; the port copied it into `buf`,
    /// truncated.
    #[test]
    fn face_name_returns_a_raw_record_longer_than_the_buffer() -> Result<(), Error> {
        let lib = Library::init()?;
        // A Macintosh Roman family record of 1500 bytes.
        let long = vec![b'L'; 1500];
        let data = font_with_names(&[(1, 0, 0, 1, long.clone())]);
        let mut face = Face::init(lib, &data, opts(size(14.0, 96, 96)))?;
        let mut buf = [0u8; 1024];
        assert_eq!(face.name(&mut buf)?, long.as_slice());
        face.deinit();
        Ok(())
    }

    /// UD190 (R13): a Windows Unicode family record whose UTF-8 form does not
    /// fit the caller's buffer. The reference's `std.unicode.utf16LeToUtf8`
    /// panics there in Debug and ReleaseSafe and writes past the buffer in
    /// ReleaseFast. Measured with Zig 0.16.0 on aarch64-macos, with a copy of
    /// `freetype.zig:170-178` and 1024 canary bytes after the buffer:
    /// - 256-byte buffer (Collection's synthetic bold/italic log,
    ///   `DeferredFace.name` callers): from 86 x U+6C34, 129 x U+00E9 or
    ///   257 x 'L';
    /// - 1024-byte buffer: from 342 x U+6C34 (ASCII and 2-byte characters
    ///   cannot overflow it, since the record holds at most 512 units).
    ///
    /// The port answers with the raw record, the reference's own fallback for
    /// a name it cannot decode. Each boundary is checked on both sides: one
    /// character fewer is decoded.
    #[test]
    fn face_name_utf16_record_too_long_for_the_buffer_is_returned_raw() -> Result<(), Error> {
        let lib = Library::init()?;
        let utf16be =
            |s: &str| -> Vec<u8> { s.encode_utf16().flat_map(|u| u.to_be_bytes()).collect() };

        // (character, count, buffer size, decoded)
        let cases: [(&str, usize, usize, bool); 8] = [
            ("水", 85, 256, true),
            ("水", 86, 256, false),
            ("é", 128, 256, true),
            ("é", 129, 256, false),
            ("L", 256, 256, true),
            ("L", 257, 256, false),
            ("水", 341, 1024, true),
            ("水", 342, 1024, false),
        ];
        for (ch, n, buf_len, decoded) in cases {
            let buf = &mut vec![0u8; buf_len][..];
            let name = ch.repeat(n);
            let record = utf16be(&name);
            // Precondition: the record takes the decode path (even length, at
            // most 1024 bytes).
            assert!(record.len() % 2 == 0 && record.len() <= 1024);
            let data = font_with_names(&[(3, 1, 0x409, 1, record.clone())]);
            let mut face = Face::init(lib, &data, opts(size(14.0, 96, 96)))?;
            let got = face.name(buf)?.to_vec();
            assert_eq!(decoded, name.len() <= buf.len(), "precondition: {n} x {ch}");
            if decoded {
                assert_eq!(got, name.as_bytes(), "{n} x {ch} is decoded");
            } else {
                assert_eq!(got, record, "{n} x {ch} returns the raw record");
            }
            face.deinit();
        }
        Ok(())
    }

    // test "face name"
    #[test]
    fn face_name() -> Result<(), Error> {
        let embedded_variable = embedded::VARIABLE;

        let mut lib: Library = Library::init()?;
        // defer lib.deinit();

        {
            let mut face: Face = Face::init(
                lib,
                embedded_variable,
                opts(DesiredSize {
                    points: 14.0,
                    xdpi: super::super::DEFAULT_DPI,
                    ydpi: super::super::DEFAULT_DPI,
                }),
            )?;
            // defer face.deinit();

            let mut buf = [0u8; 1024];
            let actual = face.name(&mut buf)?;

            assert_eq!(b"JetBrains Mono".as_slice(), actual);
            face.deinit();
        }

        {
            let mut face: Face = Face::init(
                lib,
                embedded::INCONSOLATA,
                opts(DesiredSize {
                    points: 14.0,
                    xdpi: super::super::DEFAULT_DPI,
                    ydpi: super::super::DEFAULT_DPI,
                }),
            )?;
            // defer face.deinit();

            let mut buf = [0u8; 1024];
            let actual = face.name(&mut buf)?;

            assert_eq!(b"Inconsolata".as_slice(), actual);
            face.deinit();
        }

        lib.deinit();
        Ok(())
    }

    // test {} (unnamed): render all visible ASCII + resize check.
    #[test]
    fn render_and_resize() -> Result<(), Error> {
        let test_font = embedded::INCONSOLATA;
        let alloc = Global;

        let mut lib = Library::init()?;
        // defer lib.deinit();

        let mut atlas = Atlas::init(alloc, 512, Format::Grayscale)?;
        // defer atlas.deinit(alloc);

        let mut ft_font = Face::init(lib, test_font, opts(size(12.0, 96, 96)))?;
        // defer ft_font.deinit();

        // Generate all visible ASCII
        let mut i: u8 = 32;
        while i < 127 {
            ft_font.render_glyph(
                alloc,
                &mut atlas,
                ft_font.glyph_index(i as u32).unwrap(),
                RenderOptions {
                    grid_metrics: Metrics::calc(ft_font.get_metrics()),
                    ..Default::default()
                },
            )?;
            i += 1;
        }

        // Test resizing
        {
            let g1 = ft_font.render_glyph(
                alloc,
                &mut atlas,
                ft_font.glyph_index('A' as u32).unwrap(),
                RenderOptions {
                    grid_metrics: Metrics::calc(ft_font.get_metrics()),
                    ..Default::default()
                },
            )?;
            assert_eq!(11u32, g1.height);

            ft_font.set_size(opts(size(24.0, 96, 96)))?;
            let g2 = ft_font.render_glyph(
                alloc,
                &mut atlas,
                ft_font.glyph_index('A' as u32).unwrap(),
                RenderOptions {
                    grid_metrics: Metrics::calc(ft_font.get_metrics()),
                    ..Default::default()
                },
            )?;
            assert_eq!(21u32, g2.height);
        }

        ft_font.deinit();
        atlas.deinit(alloc);
        lib.deinit();
        Ok(())
    }

    // test "color emoji"
    #[test]
    fn color_emoji() -> Result<(), Error> {
        let alloc = Global;
        let test_font = embedded::EMOJI;

        let mut lib = Library::init()?;
        // defer lib.deinit();

        let mut atlas = Atlas::init(alloc, 512, Format::Bgra)?;
        // defer atlas.deinit(alloc);

        let mut ft_font = Face::init(lib, test_font, opts(size(12.0, 96, 96)))?;
        // defer ft_font.deinit();

        ft_font.render_glyph(
            alloc,
            &mut atlas,
            ft_font.glyph_index('🥸' as u32).unwrap(),
            RenderOptions {
                grid_metrics: Metrics::calc(ft_font.get_metrics()),
                ..Default::default()
            },
        )?;

        // Make sure this glyph has color
        {
            assert!(ft_font.has_color());
            let glyph_id = ft_font.glyph_index('🥸' as u32).unwrap();
            assert!(ft_font.is_color_glyph(glyph_id));
        }

        ft_font.deinit();
        atlas.deinit(alloc);
        lib.deinit();
        Ok(())
    }

    // test "mono to bgra"
    #[test]
    fn mono_to_bgra() -> Result<(), Error> {
        let alloc = Global;
        let test_font = embedded::EMOJI;

        let mut lib = Library::init()?;
        // defer lib.deinit();

        let mut atlas = Atlas::init(alloc, 512, Format::Bgra)?;
        // defer atlas.deinit(alloc);

        let mut ft_font = Face::init(lib, test_font, opts(size(12.0, 72, 72)))?;
        // defer ft_font.deinit();

        // glyph 3 is mono in Noto
        ft_font.render_glyph(
            alloc,
            &mut atlas,
            3,
            RenderOptions {
                grid_metrics: Metrics::calc(ft_font.get_metrics()),
                ..Default::default()
            },
        )?;

        ft_font.deinit();
        atlas.deinit(alloc);
        lib.deinit();
        Ok(())
    }

    // test "svg font table"
    #[test]
    fn svg_font_table() -> Result<(), Error> {
        let alloc = Global;
        let test_font = embedded::JULIA_MONO;

        let mut lib = Library::init()?;
        // defer lib.deinit();

        let mut face = Face::init(lib, test_font, opts(size(12.0, 72, 72)))?;
        // defer face.deinit();

        let table = face.copy_table(alloc, b"SVG ")?.unwrap();
        // defer alloc.free(table);

        assert_eq!(430, table.len());

        face.deinit();
        lib.deinit();
        Ok(())
    }

    // const terminus_i = ...
    const TERMINUS_I: &str = concat!(
        "........\n",
        "........\n",
        "...#....\n",
        "...#....\n",
        "........\n",
        "..##....\n",
        "...#....\n",
        "...#....\n",
        "...#....\n",
        "...#....\n",
        "...#....\n",
        "..###...\n",
        "........\n",
        "........\n",
        "........\n",
        "........",
    );
    // Including the newline
    const TERMINUS_I_PITCH: usize = 9;

    // test "bitmap glyph"
    #[test]
    fn bitmap_glyph() -> Result<(), Error> {
        let alloc = Global;
        let test_font = embedded::TERMINUS_TTF;

        let mut lib = Library::init()?;
        // defer lib.deinit();

        let mut atlas = Atlas::init(alloc, 512, Format::Grayscale)?;
        // defer atlas.deinit(alloc);

        // Any glyph at 12pt @ 96 DPI is a bitmap
        let mut ft_font = Face::init(lib, test_font, opts(size(12.0, 96, 96)))?;
        // defer ft_font.deinit();

        // glyph 77 = 'i'
        let glyph = ft_font.render_glyph(
            alloc,
            &mut atlas,
            77,
            RenderOptions {
                grid_metrics: Metrics::calc(ft_font.get_metrics()),
                ..Default::default()
            },
        )?;

        // should render crisp
        assert_eq!(8, glyph.width);
        assert_eq!(16, glyph.height);
        let terminus_i = TERMINUS_I.as_bytes();
        for y in 0..glyph.height as usize {
            for x in 0..glyph.width as usize {
                let pixel = terminus_i[y * TERMINUS_I_PITCH + x];
                assert_eq!(
                    if pixel == b'#' { 255u8 } else { 0u8 },
                    atlas.data()[(glyph.atlas_y as usize + y) * atlas.size() as usize
                        + (glyph.atlas_x as usize + x)],
                );
            }
        }

        ft_font.deinit();
        atlas.deinit(alloc);
        lib.deinit();
        Ok(())
    }

    // Expected pixel pattern for Spleen 8x16 'A' (glyph index from char 'A')
    // Derived from BDF BITMAP data: 00,00,7C,C6,C6,C6,FE,C6,C6,C6,C6,C6,00,00,00,00
    const SPLEEN_A: &str = concat!(
        "........\n",
        "........\n",
        ".#####..\n",
        "##...##.\n",
        "##...##.\n",
        "##...##.\n",
        "#######.\n",
        "##...##.\n",
        "##...##.\n",
        "##...##.\n",
        "##...##.\n",
        "##...##.\n",
        "........\n",
        "........\n",
        "........\n",
        "........",
    );
    // Including the newline
    const SPLEEN_A_PITCH: usize = 9;
    // Test parameters for bitmap font tests
    const SPLEEN_TEST_POINT_SIZE: f32 = 12.0;
    const SPLEEN_TEST_DPI: u16 = 96;

    // test "bitmap glyph BDF"
    #[test]
    fn bitmap_glyph_bdf() -> Result<(), Error> {
        let alloc = Global;
        let test_font = embedded::SPLEEN_BDF;

        let mut lib = Library::init()?;
        // defer lib.deinit();

        let mut atlas = Atlas::init(alloc, 512, Format::Grayscale)?;
        // defer atlas.deinit(alloc);

        // Spleen 8x16 is a pure bitmap font at 16px height
        let mut ft_font = Face::init(
            lib,
            test_font,
            opts(size(
                SPLEEN_TEST_POINT_SIZE,
                SPLEEN_TEST_DPI,
                SPLEEN_TEST_DPI,
            )),
        )?;
        // defer ft_font.deinit();

        // Get glyph index for 'A'
        let glyph_index = ft_font
            .glyph_index('A' as u32)
            .ok_or(Error::GlyphNotFound)?;

        let glyph = ft_font.render_glyph(
            alloc,
            &mut atlas,
            glyph_index,
            RenderOptions {
                grid_metrics: Metrics::calc(ft_font.get_metrics()),
                ..Default::default()
            },
        )?;

        // Verify dimensions match Spleen 8x16
        assert_eq!(8, glyph.width);
        assert_eq!(16, glyph.height);

        // Verify pixel-perfect rendering
        let spleen_a = SPLEEN_A.as_bytes();
        for y in 0..glyph.height as usize {
            for x in 0..glyph.width as usize {
                let pixel = spleen_a[y * SPLEEN_A_PITCH + x];
                assert_eq!(
                    if pixel == b'#' { 255u8 } else { 0u8 },
                    atlas.data()[(glyph.atlas_y as usize + y) * atlas.size() as usize
                        + (glyph.atlas_x as usize + x)],
                );
            }
        }

        ft_font.deinit();
        atlas.deinit(alloc);
        lib.deinit();
        Ok(())
    }

    // test "bitmap glyph PCF"
    #[test]
    fn bitmap_glyph_pcf() -> Result<(), Error> {
        let alloc = Global;
        let test_font = embedded::SPLEEN_PCF;

        let mut lib = Library::init()?;
        // defer lib.deinit();

        let mut atlas = Atlas::init(alloc, 512, Format::Grayscale)?;
        // defer atlas.deinit(alloc);

        let mut ft_font = Face::init(
            lib,
            test_font,
            opts(size(
                SPLEEN_TEST_POINT_SIZE,
                SPLEEN_TEST_DPI,
                SPLEEN_TEST_DPI,
            )),
        )?;
        // defer ft_font.deinit();

        let glyph_index = ft_font
            .glyph_index('A' as u32)
            .ok_or(Error::GlyphNotFound)?;

        let glyph = ft_font.render_glyph(
            alloc,
            &mut atlas,
            glyph_index,
            RenderOptions {
                grid_metrics: Metrics::calc(ft_font.get_metrics()),
                ..Default::default()
            },
        )?;

        assert_eq!(8, glyph.width);
        assert_eq!(16, glyph.height);

        let spleen_a = SPLEEN_A.as_bytes();
        for y in 0..glyph.height as usize {
            for x in 0..glyph.width as usize {
                let pixel = spleen_a[y * SPLEEN_A_PITCH + x];
                assert_eq!(
                    if pixel == b'#' { 255u8 } else { 0u8 },
                    atlas.data()[(glyph.atlas_y as usize + y) * atlas.size() as usize
                        + (glyph.atlas_x as usize + x)],
                );
            }
        }

        ft_font.deinit();
        atlas.deinit(alloc);
        lib.deinit();
        Ok(())
    }

    // test "bitmap glyph OTB"
    #[test]
    fn bitmap_glyph_otb() -> Result<(), Error> {
        let alloc = Global;
        let test_font = embedded::SPLEEN_OTB;

        let mut lib = Library::init()?;
        // defer lib.deinit();

        let mut atlas = Atlas::init(alloc, 512, Format::Grayscale)?;
        // defer atlas.deinit(alloc);

        let mut ft_font = Face::init(
            lib,
            test_font,
            opts(size(
                SPLEEN_TEST_POINT_SIZE,
                SPLEEN_TEST_DPI,
                SPLEEN_TEST_DPI,
            )),
        )?;
        // defer ft_font.deinit();

        let glyph_index = ft_font
            .glyph_index('A' as u32)
            .ok_or(Error::GlyphNotFound)?;

        let glyph = ft_font.render_glyph(
            alloc,
            &mut atlas,
            glyph_index,
            RenderOptions {
                grid_metrics: Metrics::calc(ft_font.get_metrics()),
                ..Default::default()
            },
        )?;

        assert_eq!(8, glyph.width);
        assert_eq!(16, glyph.height);

        let spleen_a = SPLEEN_A.as_bytes();
        for y in 0..glyph.height as usize {
            for x in 0..glyph.width as usize {
                let pixel = spleen_a[y * SPLEEN_A_PITCH + x];
                assert_eq!(
                    if pixel == b'#' { 255u8 } else { 0u8 },
                    atlas.data()[(glyph.atlas_y as usize + y) * atlas.size() as usize
                        + (glyph.atlas_x as usize + x)],
                );
            }
        }

        ft_font.deinit();
        atlas.deinit(alloc);
        lib.deinit();
        Ok(())
    }

    /// KAT: the `stbir_resize_uint8` seam on a NON-UNIT bitmap-strike resize.
    ///
    /// The four `bitmap_glyph*` tests above all resize at scale 1.0, where every
    /// resampler agrees (proved: substituting nearest neighbour leaves them green),
    /// so they cannot witness resampler parity. This one does: Terminus' 8x16 strike
    /// (pinned by `bitmap_glyph`) is stretched to 16x17 and 8x17, and the resulting
    /// atlas bytes are compared against fixtures generated from the vendored C by the
    /// D10 consensus build (`.port/tools/stb-resize-oracle/`, provenance in
    /// `src/testdata/README.stb-resize.md`). Both dimension asserts are load-bearing:
    /// if the constraint math ever collapses these back to 8x16 the test must fail
    /// loudly rather than quietly become vacuous again.
    #[test]
    fn stb_resize_kat_bitmap_strike_grayscale() -> Result<(), Error> {
        use rustty_font::glyph::{Align, Constraint};

        const KAT_16X17: &[u8] = include_bytes!("../testdata/stb_resize_terminus_i_gray_16x17.bin");
        const KAT_8X17: &[u8] = include_bytes!("../testdata/stb_resize_terminus_i_gray_8x17.bin");

        let alloc = Global;
        let mut lib = Library::init()?;
        let mut atlas = Atlas::init(alloc, 512, Format::Grayscale)?;
        let mut ft_font = Face::init(lib, embedded::TERMINUS_TTF, opts(size(12.0, 96, 96)))?;
        let metrics = Metrics::calc(ft_font.get_metrics());

        // (constraint_width, expected w, expected h, fixture)
        let cases: [(u8, u32, u32, &[u8]); 2] = [(2, 16, 17, KAT_16X17), (1, 8, 17, KAT_8X17)];

        for (cw, ew, eh, expected) in cases {
            // glyph 77 = 'i' (same glyph the `bitmap_glyph` test pins at 8x16).
            let g = ft_font.render_glyph(
                alloc,
                &mut atlas,
                77,
                RenderOptions {
                    grid_metrics: metrics,
                    constraint_width: cw,
                    constraint: Constraint {
                        size: ConstraintSize::Stretch,
                        align_horizontal: Align::Center,
                        align_vertical: Align::Center,
                        ..Constraint::none
                    },
                    ..Default::default()
                },
            )?;

            assert_eq!(
                (ew, eh),
                (g.width, g.height),
                "constraint_width={cw}: target size"
            );
            assert_ne!(
                (8u32, 16u32),
                (g.width, g.height),
                "constraint_width={cw}: this must be a NON-identity resize or the KAT is vacuous"
            );
            assert_eq!(
                expected.len(),
                (g.width * g.height) as usize,
                "fixture length"
            );

            let mut actual = Vec::with_capacity(expected.len());
            for y in 0..g.height as usize {
                for x in 0..g.width as usize {
                    actual.push(
                        atlas.data()[(g.atlas_y as usize + y) * atlas.size() as usize
                            + (g.atlas_x as usize + x)],
                    );
                }
            }
            assert_eq!(
                expected,
                &actual[..],
                "constraint_width={cw}: resized bytes differ from the stb consensus fixture"
            );
        }

        ft_font.deinit();
        atlas.deinit(alloc);
        lib.deinit();
        Ok(())
    }

    /// KAT: the 4-channel (BGRA) side of the resize seam, on a large DOWNSAMPLE.
    ///
    /// `color_emoji` above renders the same face but at scale 1.0 (136x128 -> 136x128)
    /// because it derives grid metrics from the emoji face itself, whose cell is as
    /// large as its strike. Real rendering takes grid metrics from the PRIMARY text
    /// face, which is what makes the constraint bite; that is reproduced here, giving
    /// a 136x128 -> 16x15 / 8x7 downsample through `num_channels = 4`.
    ///
    /// This is the STRONG discriminator of the two KATs: unlike the grayscale strike
    /// (mono, so stb's in/out are 0/1 and a wrong resampler's fractional output rounds
    /// back — see `src/testdata/README.stb-resize.md` TRAP 2), this source is genuine
    /// 8-bit BGRA, and a large downsample selects stb's downsampling kernel.
    #[test]
    fn stb_resize_kat_bitmap_strike_bgra() -> Result<(), Error> {
        use rustty_font::glyph::{Align, Constraint};

        const KAT_16X15: &[u8] = include_bytes!("../testdata/stb_resize_emoji_bgra_16x15.bin");
        const KAT_8X7: &[u8] = include_bytes!("../testdata/stb_resize_emoji_bgra_8x7.bin");

        let alloc = Global;
        let mut lib = Library::init()?;
        let mut atlas = Atlas::init(alloc, 1024, Format::Bgra)?;

        // Grid metrics come from the PRIMARY (outline) face, as at runtime. Taking them
        // from the emoji face instead makes the constraint a NO-OP (all nine
        // constraint_width x ConstraintSize combinations return 136x128) — that is the
        // trap `color_emoji` fell into.
        let mut primary = Face::init(lib, embedded::INCONSOLATA, opts(size(12.0, 96, 96)))?;
        let metrics = Metrics::calc(primary.get_metrics());
        primary.deinit();

        let mut emoji = Face::init(lib, embedded::EMOJI, opts(size(12.0, 96, 96)))?;
        let idx = emoji
            .glyph_index(0x1F600)
            .expect("U+1F600 must be in NotoColorEmoji");

        let cases: [(u8, u32, u32, &[u8]); 2] = [(2, 16, 15, KAT_16X15), (1, 8, 7, KAT_8X7)];

        for (cw, ew, eh, expected) in cases {
            let g = emoji.render_glyph(
                alloc,
                &mut atlas,
                idx,
                RenderOptions {
                    grid_metrics: metrics,
                    constraint_width: cw,
                    constraint: Constraint {
                        size: ConstraintSize::Cover,
                        align_horizontal: Align::Center,
                        align_vertical: Align::Center,
                        ..Constraint::none
                    },
                    ..Default::default()
                },
            )?;

            assert_eq!(
                (ew, eh),
                (g.width, g.height),
                "constraint_width={cw}: target size"
            );
            assert_ne!(
                (136u32, 128u32),
                (g.width, g.height),
                "constraint_width={cw}: must be a real downsample, not the identity path"
            );
            assert_eq!(
                expected.len(),
                (g.width * g.height * 4) as usize,
                "fixture length (BGRA)"
            );

            let mut actual = Vec::with_capacity(expected.len());
            for y in 0..g.height as usize {
                for x in 0..g.width as usize {
                    let i = ((g.atlas_y as usize + y) * atlas.size() as usize
                        + (g.atlas_x as usize + x))
                        * 4;
                    actual.extend_from_slice(&atlas.data()[i..i + 4]);
                }
            }
            assert_eq!(
                expected,
                &actual[..],
                "constraint_width={cw}: resized BGRA bytes differ from the stb consensus fixture"
            );
        }

        emoji.deinit();
        atlas.deinit(alloc);
        lib.deinit();
        Ok(())
    }

    /// The negative control for the two resize KATs, made PERMANENT and
    /// machine-checked instead of procedural.
    ///
    /// Tasks 4-5 established that the KATs discriminate by having a human re-apply a
    /// nearest-neighbour substitution at the `stbir_resize_uint8` call site and watch
    /// them go red. That evidence evaporates after the commit. This test bakes it in:
    /// it recomputes nearest-neighbour and bilinear resamplings of the SAME source
    /// strikes inside the test and asserts the fixtures REJECT them, with the measured
    /// delta magnitudes as the assertion. If someone later loosens a KAT to a
    /// tolerance, this fails.
    ///
    /// It also asserts the tolerated cross-toolchain envelope is DISJOINT from a wrong
    /// resampler: the envelope D10 allows for ghostty's `ReleaseFast` build is max
    /// delta 1 on <= 2 % of bytes, and a +1-on-2 % perturbation of a fixture is shown
    /// to satisfy it — so the two bands cannot overlap.
    ///
    /// THRESHOLDS ARE PER-CASE AND MEASURED, not uniform. The grayscale strike is MONO
    /// (num_grays = 2, so stb reads and writes 0/1 and `freetype.rs` multiplies by 255
    /// afterwards — see `src/testdata/README.stb-resize.md` TRAP 2), which rounds a
    /// wrong resampler's fractional output back onto the source values. So the
    /// grayscale case has a small differing-byte COUNT at a huge delta, while the
    /// 8-bit BGRA case moves most of the image:
    ///
    ///   gray 16x17  vs nearest   11/272 ( 4.0 %) maxdelta=255
    ///               vs bilinear   2/272 ( 0.7 %) maxdelta=255
    ///   BGRA 16x15  vs nearest  597/960 (62.2 %) maxdelta=210
    ///               vs bilinear 575/960 (59.9 %) maxdelta=108
    ///
    /// (Those BGRA figures are 1-3 bytes off the ones in
    /// `src/testdata/README.stb-resize.md`, which derives the source strike from the
    /// dumped PRE-resize FT bitmap. This test recovers it from the atlas after an
    /// UNCONSTRAINED render, which still passes through the resize call at scale 1.0 —
    /// and stb at scale 1.0 is not bit-exactly the identity for 4 interleaved channels.
    /// Both sets of numbers are real; the margins are unaffected.)
    ///
    /// (For completeness: gray 8x17 vs bilinear is 0/136 — INDISTINGUISHABLE. That is
    /// why the BGRA KAT exists and why this test asserts the >50 %-of-bytes margin only
    /// on the BGRA case.)
    #[test]
    fn stb_resize_kat_rejects_nearest_and_bilinear() -> Result<(), Error> {
        const KAT_GRAY_16X17: &[u8] =
            include_bytes!("../testdata/stb_resize_terminus_i_gray_16x17.bin");
        const KAT_BGRA_16X15: &[u8] = include_bytes!("../testdata/stb_resize_emoji_bgra_16x15.bin");

        /// The exact substitution that left all 9 pre-WS11 tests green.
        fn nearest(src: &[u8], sw: usize, sh: usize, ow: usize, oh: usize, c: usize) -> Vec<u8> {
            let mut out = Vec::with_capacity(ow * oh * c);
            for oy in 0..oh {
                let sy = oy * sh / oh;
                for ox in 0..ow {
                    let sx = ox * sw / ow;
                    for ch in 0..c {
                        out.push(src[(sy * sw + sx) * c + ch]);
                    }
                }
            }
            out
        }

        /// A plausible near-miss: pixel-centre mapping, clamped edges, 2 taps. stb's
        /// default is a wider Mitchell/Catmull-Rom kernel.
        fn bilinear(src: &[u8], sw: usize, sh: usize, ow: usize, oh: usize, c: usize) -> Vec<u8> {
            let at = |x: isize, y: isize, ch: usize| -> f64 {
                let x = x.clamp(0, sw as isize - 1) as usize;
                let y = y.clamp(0, sh as isize - 1) as usize;
                src[(y * sw + x) * c + ch] as f64
            };
            let mut out = Vec::with_capacity(ow * oh * c);
            for oy in 0..oh {
                let fy = (oy as f64 + 0.5) * sh as f64 / oh as f64 - 0.5;
                let y0 = fy.floor();
                let wy = fy - y0;
                for ox in 0..ow {
                    let fx = (ox as f64 + 0.5) * sw as f64 / ow as f64 - 0.5;
                    let x0 = fx.floor();
                    let wx = fx - x0;
                    for ch in 0..c {
                        let (xi, yi) = (x0 as isize, y0 as isize);
                        let v = at(xi, yi, ch) * (1.0 - wx) * (1.0 - wy)
                            + at(xi + 1, yi, ch) * wx * (1.0 - wy)
                            + at(xi, yi + 1, ch) * (1.0 - wx) * wy
                            + at(xi + 1, yi + 1, ch) * wx * wy;
                        out.push(v.round().clamp(0.0, 255.0) as u8);
                    }
                }
            }
            out
        }

        fn delta(a: &[u8], b: &[u8]) -> (usize, u8) {
            assert_eq!(a.len(), b.len(), "delta() operands must be the same length");
            let n = a.iter().zip(b).filter(|(x, y)| x != y).count();
            let mx = a
                .iter()
                .zip(b)
                .map(|(x, y)| x.abs_diff(*y))
                .max()
                .unwrap_or(0);
            (n, mx)
        }

        // The tolerated cross-toolchain envelope, verbatim from gate D10.
        const ENVELOPE_MAX_DELTA: u8 = 1;
        const ENVELOPE_MAX_FRACTION: f64 = 0.03;
        // The margin a WRONG resampler must clear to be unambiguously outside it.
        const MARGIN_MIN_DELTA: u8 = 10;

        let alloc = Global;
        let mut lib = Library::init()?;

        // ---------- grayscale: recover the 8x16 source strike -------------------
        // The atlas holds it post-normalisation (0/255). stb itself sees the MONO
        // 0/1 values, so a substituted resampler would too: divide by 255, resample,
        // then re-apply the x255 factor, exactly as `freetype.rs` does.
        let mut gray_atlas = Atlas::init(alloc, 512, Format::Grayscale)?;
        let mut terminus = Face::init(lib, embedded::TERMINUS_TTF, opts(size(12.0, 96, 96)))?;
        let src_glyph = terminus.render_glyph(
            alloc,
            &mut gray_atlas,
            77,
            RenderOptions {
                grid_metrics: Metrics::calc(terminus.get_metrics()),
                ..Default::default()
            },
        )?;
        assert_eq!(
            (8, 16),
            (src_glyph.width, src_glyph.height),
            "source strike must be 8x16"
        );
        let mut gray_src = Vec::with_capacity(8 * 16);
        for y in 0..16usize {
            for x in 0..8usize {
                let v = gray_atlas.data()[(src_glyph.atlas_y as usize + y)
                    * gray_atlas.size() as usize
                    + (src_glyph.atlas_x as usize + x)];
                gray_src.push(v / 255); // back to the mono 0/1 values stb sees
            }
        }
        terminus.deinit();
        gray_atlas.deinit(alloc);

        for (name, wrong) in [
            ("nearest", nearest(&gray_src, 8, 16, 16, 17, 1)),
            ("bilinear", bilinear(&gray_src, 8, 16, 16, 17, 1)),
        ] {
            let wrong: Vec<u8> = wrong.iter().map(|v| v.saturating_mul(255)).collect();
            let (n, mx) = delta(KAT_GRAY_16X17, &wrong);
            assert_ne!(
                KAT_GRAY_16X17,
                &wrong[..],
                "gray 16x17: {name} must NOT equal the fixture"
            );
            assert!(
                n > 0,
                "gray 16x17 vs {name}: expected differing bytes, got 0"
            );
            assert!(
                mx >= MARGIN_MIN_DELTA,
                "gray 16x17 vs {name}: maxdelta {mx} < required margin {MARGIN_MIN_DELTA}                  (measured: 255 for both)"
            );
            // Disjoint from the tolerated envelope.
            assert!(
                !(mx <= ENVELOPE_MAX_DELTA
                    && (n as f64) <= ENVELOPE_MAX_FRACTION * KAT_GRAY_16X17.len() as f64),
                "gray 16x17 vs {name}: {n} bytes at maxdelta {mx} falls INSIDE the                  cross-toolchain envelope — the envelope cannot discriminate"
            );
        }

        // ---------- BGRA: recover the 136x128 source strike ---------------------
        // Unconstrained render, metrics from the emoji face itself — that is the
        // identity path `color_emoji` takes, and here it is exactly what we want: the
        // untouched source strike.
        let mut bgra_atlas = Atlas::init(alloc, 512, Format::Bgra)?;
        let mut emoji = Face::init(lib, embedded::EMOJI, opts(size(12.0, 96, 96)))?;
        let idx = emoji
            .glyph_index(0x1F600)
            .expect("U+1F600 must be in NotoColorEmoji");
        let src_glyph = emoji.render_glyph(
            alloc,
            &mut bgra_atlas,
            idx,
            RenderOptions {
                grid_metrics: Metrics::calc(emoji.get_metrics()),
                ..Default::default()
            },
        )?;
        assert_eq!(
            (136, 128),
            (src_glyph.width, src_glyph.height),
            "unconstrained emoji render must be the raw 136x128 strike"
        );
        let mut bgra_src = Vec::with_capacity(136 * 128 * 4);
        for y in 0..128usize {
            for x in 0..136usize {
                let i = ((src_glyph.atlas_y as usize + y) * bgra_atlas.size() as usize
                    + (src_glyph.atlas_x as usize + x))
                    * 4;
                bgra_src.extend_from_slice(&bgra_atlas.data()[i..i + 4]);
            }
        }
        emoji.deinit();
        bgra_atlas.deinit(alloc);

        for (name, wrong) in [
            ("nearest", nearest(&bgra_src, 136, 128, 16, 15, 4)),
            ("bilinear", bilinear(&bgra_src, 136, 128, 16, 15, 4)),
        ] {
            let (n, mx) = delta(KAT_BGRA_16X15, &wrong);
            let frac = n as f64 / KAT_BGRA_16X15.len() as f64;
            assert_ne!(
                KAT_BGRA_16X15,
                &wrong[..],
                "BGRA 16x15: {name} must NOT equal the fixture"
            );
            assert!(
                frac > 0.5,
                "BGRA 16x15 vs {name}: only {n}/{} bytes ({:.1} %) differ, expected >50 % (measured: 62.2 % nearest, 59.9 % bilinear)",
                KAT_BGRA_16X15.len(),
                frac * 100.0
            );
            assert!(
                mx >= MARGIN_MIN_DELTA,
                "BGRA 16x15 vs {name}: maxdelta {mx} < required margin {MARGIN_MIN_DELTA}                  (measured: 210 nearest, 108 bilinear)"
            );
            assert!(
                !(mx <= ENVELOPE_MAX_DELTA && frac <= ENVELOPE_MAX_FRACTION),
                "BGRA 16x15 vs {name}: {n} bytes at maxdelta {mx} falls INSIDE the                  cross-toolchain envelope — the envelope cannot discriminate"
            );
        }

        // ---------- and the envelope really does admit cross-toolchain noise ----
        // A fixture perturbed by +1 on 2 % of its bytes MUST satisfy the envelope,
        // which is what makes "outside the envelope" a meaningful rejection above
        // rather than a threshold nothing could ever meet.
        let mut noisy = KAT_BGRA_16X15.to_vec();
        let step = (1.0 / ENVELOPE_MAX_FRACTION) as usize;
        for i in (0..noisy.len()).step_by(step) {
            noisy[i] = noisy[i].saturating_add(1);
        }
        let (n, mx) = delta(KAT_BGRA_16X15, &noisy);
        let frac = n as f64 / noisy.len() as f64;
        assert!(
            mx <= ENVELOPE_MAX_DELTA && frac <= ENVELOPE_MAX_FRACTION,
            "a +1-on-2 % perturbation must satisfy the envelope, got {n} bytes ({:.1} %) at maxdelta {mx}",
            frac * 100.0
        );

        lib.deinit();
        Ok(())
    }

    // -----------------------------------------------------------------------
    // task-14c-4 (S91-F8): the restored `if (comptime builtin.mode == .Debug)`
    // variation-axes dump. RUSTTY-ONLY tests (the reference cannot observe
    // `std.log` from a test), but they observe the reference's OWN message
    // text, level and `.font_face` scope.
    //
    // No device or environment is required: both drive embedded font BYTES
    // through FreeType, so there is no skip arm to make vacuous and no
    // `RUSTTY_REQUIRE_*` strict-mode pivot is needed here.
    // -----------------------------------------------------------------------

    /// The RELEASE arm. `debug_mode == false` is the port of
    /// `comptime builtin.mode == .Debug` being false, and it must emit NOTHING
    /// even for a font that HAS variation axes — otherwise a release build of
    /// rustty would log where the reference is silent.
    /// Task R13-P (`NEW-FINDINGS.md` N53): the reference's
    /// `errdefer hb_font.destroy()` / `errdefer lib.alloc.destroy(ft_mutex)`
    /// (`face/freetype.zig:100-105`) cover the fallible variation-axes step at
    /// `:120-126`. The port's `?` dropped the `Face` instead, leaking the
    /// HarfBuzz font and the boxed mutex. Proven POSITIVELY for the HarfBuzz
    /// font: a user-data destroy callback runs only when the font is destroyed.
    /// The boxed mutex is proven freed by
    /// `a_failed_variation_axes_step_frees_the_ft_mutex` below.
    #[test]
    fn a_failed_variation_axes_step_destroys_the_harfbuzz_font() -> Result<(), Error> {
        use core::sync::atomic::{AtomicUsize, Ordering};

        static DESTROYED: AtomicUsize = AtomicUsize::new(0);
        unsafe extern "C" fn on_destroy(data: *mut core::ffi::c_void) {
            DESTROYED.fetch_add(1, Ordering::SeqCst);
            // SAFETY: the only `data` ever attached is the leaked box below.
            drop(unsafe { Box::from_raw(data.cast::<u8>()) });
        }

        // A real face, to borrow the FT face from. `base` owns it and frees it
        // in `deinit`, exactly as `initFile`/`init`'s `errdefer face.deinit()`
        // owns it around `initFace`.
        let lib = Library::init()?;
        let mut base = Face::init(lib, embedded::VARIABLE, opts(size(12.0, 96, 96)))?;

        let before = DESTROYED.load(Ordering::SeqCst);
        let got = Face::init_face_with(base.lib, base.face, opts(size(12.0, 96, 96)), |f| {
            let key: *mut harfbuzz_sys::hb_user_data_key_t =
                // SAFETY: the key is an opaque token HarfBuzz only compares by
                // address; zeroed is a valid value for it.
                Box::into_raw(Box::new(unsafe {
                    core::mem::zeroed::<harfbuzz_sys::hb_user_data_key_t>()
                }));
            let data = Box::into_raw(Box::new(0u8)).cast::<core::ffi::c_void>();
            // SAFETY: `f.hb_font` is live for the call, and `key`/`data` are
            // leaked so they outlive the font.
            let ok = unsafe {
                harfbuzz_sys::hb_font_set_user_data(
                    f.hb_font.handle,
                    key,
                    data,
                    Some(on_destroy),
                    1,
                )
            };
            assert_ne!(0, ok, "the destroy probe must attach");
            Err(Error::OutOfMemory)
        });

        assert!(
            matches!(got, Err(Error::OutOfMemory)),
            "the failure propagates"
        );
        assert_eq!(
            before + 1,
            DESTROYED.load(Ordering::SeqCst),
            "the HarfBuzz font is destroyed on the error path, not leaked"
        );

        base.deinit();
        Ok(())
    }

    // Task R13-P2 (N53): the probe for the OTHER errdefer on that arm,
    // `errdefer lib.alloc.destroy(ft_mutex)`. The boxed mutex is a
    // `std::sync::Mutex<()>`, which cannot carry a drop counter, so the probe
    // watches the ALLOCATION instead: a global allocator that forwards every
    // call to `System` and, on `dealloc`, records whether the pointer it frees is
    // the one this thread asked it to watch. The counters are THREAD-LOCAL, so a
    // parallel test's frees cannot set them (libtest runs each test on its own
    // thread, and the free under test happens on the test's thread). A watched
    // address cannot be reused while its box is still live, so a later
    // allocation at the same address cannot fake the free.
    struct DeallocWatch;

    thread_local! {
        static WATCHED: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
        static WATCHED_FREED: core::cell::Cell<bool> = const { core::cell::Cell::new(false) };
    }

    unsafe impl std::alloc::GlobalAlloc for DeallocWatch {
        unsafe fn alloc(&self, layout: std::alloc::Layout) -> *mut u8 {
            unsafe { std::alloc::System.alloc(layout) }
        }
        unsafe fn dealloc(&self, ptr: *mut u8, layout: std::alloc::Layout) {
            // `try_with`: never touch a thread-local that is being torn down.
            let _ = WATCHED.try_with(|w| {
                if w.get() != 0 && w.get() == ptr as usize {
                    let _ = WATCHED_FREED.try_with(|f| f.set(true));
                }
            });
            unsafe { std::alloc::System.dealloc(ptr, layout) }
        }
        unsafe fn realloc(
            &self,
            ptr: *mut u8,
            layout: std::alloc::Layout,
            new_size: usize,
        ) -> *mut u8 {
            unsafe { std::alloc::System.realloc(ptr, layout, new_size) }
        }
        unsafe fn alloc_zeroed(&self, layout: std::alloc::Layout) -> *mut u8 {
            unsafe { std::alloc::System.alloc_zeroed(layout) }
        }
    }

    // D6-ALLOW(watching shim, not an allocator substitution): this lives inside
    // `#[cfg(test)] mod tests`, so it exists only in this crate's test binary
    // and can never enter a shipped build. Every arm forwards to
    // `std::alloc::System`, i.e. plain `malloc`/`free`, which the ASan/LSan
    // interceptors still see; `dealloc` only compares a pointer against a
    // thread-local first. Same shape as `rustty-renderer/src/link.rs`'s
    // counting shim.
    #[global_allocator]
    static DEALLOC_WATCH: DeallocWatch = DeallocWatch;

    #[test]
    fn a_failed_variation_axes_step_frees_the_ft_mutex() -> Result<(), Error> {
        // The probe needs a real allocation to watch: a zero-sized box would
        // never reach the allocator.
        assert_ne!(0, core::mem::size_of::<library::Mutex>());

        // The probe itself sees a free of the watched pointer (and only then).
        let sentinel = Box::into_raw(Box::new(library::Mutex::new(())));
        WATCHED_FREED.with(|f| f.set(false));
        WATCHED.with(|w| w.set(sentinel as usize));
        assert!(!WATCHED_FREED.with(|f| f.get()));
        // SAFETY: `sentinel` came from `Box::into_raw` just above.
        drop(unsafe { Box::from_raw(sentinel) });
        assert!(
            WATCHED_FREED.with(|f| f.get()),
            "the probe observes the free"
        );
        WATCHED.with(|w| w.set(0));

        // A real face, to borrow the FT face from (as in the test above).
        let lib = Library::init()?;
        let mut base = Face::init(lib, embedded::VARIABLE, opts(size(12.0, 96, 96)))?;

        WATCHED_FREED.with(|f| f.set(false));
        let mut watched = 0usize;
        let got = Face::init_face_with(base.lib, base.face, opts(size(12.0, 96, 96)), |f| {
            watched = f.ft_mutex as usize;
            WATCHED.with(|w| w.set(watched));
            assert!(
                !WATCHED_FREED.with(|f| f.get()),
                "the mutex is live while the face is being built"
            );
            Err(Error::OutOfMemory)
        });
        let freed = WATCHED_FREED.with(|f| f.get());
        WATCHED.with(|w| w.set(0));

        assert!(
            matches!(got, Err(Error::OutOfMemory)),
            "the failure propagates"
        );
        assert_ne!(0, watched, "the seam saw the boxed mutex");
        assert!(
            freed,
            "the boxed ft_mutex is freed on the error path, not leaked"
        );

        base.deinit();
        Ok(())
    }

    /// UD98 (R13, task R13-D): `font-variation` values whose 16.16 fixed-point
    /// form is out of range. The reference's `@intFromFloat(v.value * 65536)`
    /// traps on `inf` and `1e20` (ReleaseFast: the `c_long` max) and gives 0 for
    /// NaN (measured, Zig 0.16.0). The port stores those same saturated values,
    /// and the face still renders a glyph afterwards.
    #[test]
    fn set_variations_with_an_out_of_range_value_saturates() -> Result<(), Error> {
        let mut lib = Library::init()?;
        let mut face = Face::init(lib, embedded::VARIABLE, opts(size(12.0, 96, 96)))?;
        let wght = super::super::Id::init(b"wght");
        let coord = |face: &Face| -> Result<c::FT_Fixed, Error> {
            let mut coords: [c::FT_Fixed; 1] = [0];
            face.face.get_var_design_coordinates(&mut coords)?;
            Ok(coords[0])
        };

        // Positive control: an in-range weight is applied as given.
        face.set_variations(
            &[super::super::Variation {
                id: wght,
                value: 600.0,
            }],
            opts(size(12.0, 96, 96)),
        )?;
        assert_eq!(600 << 16, coord(&face)?);

        let alloc = Global;
        let mut atlas = Atlas::init(alloc, 512, Format::Grayscale)?;
        for (value, want) in [
            (f64::INFINITY, c::FT_Fixed::MAX),
            (1e20, c::FT_Fixed::MAX),
            (f64::NEG_INFINITY, c::FT_Fixed::MIN),
            (f64::NAN, 0),
        ] {
            face.set_variations(
                &[super::super::Variation { id: wght, value }],
                opts(size(12.0, 96, 96)),
            )?;
            assert_eq!(want, coord(&face)?, "wght={value}");
            face.render_glyph(
                alloc,
                &mut atlas,
                face.glyph_index('A' as u32).unwrap(),
                RenderOptions {
                    grid_metrics: Metrics::calc(face.get_metrics()),
                    ..Default::default()
                },
            )?;
        }

        atlas.deinit(alloc);
        face.deinit();
        lib.deinit();
        Ok(())
    }

    #[test]
    fn log_variation_axes_is_silent_when_not_debug_mode() -> Result<(), Error> {
        let mut lib: Library = Library::init()?;
        let mut face: Face = Face::init(
            lib,
            embedded::VARIABLE,
            opts(size(
                14.0,
                super::super::DEFAULT_DPI,
                super::super::DEFAULT_DPI,
            )),
        )?;

        let cap = crate::log_capture::start();
        face.log_variation_axes(false)?;
        let messages = cap.messages();
        drop(cap);

        assert_eq!(Vec::<crate::log_capture::CapturedRecord>::new(), messages);

        face.deinit();
        lib.deinit();
        Ok(())
    }

    /// The DEBUG arm, positive control: a variable font emits the header debug
    /// plus exactly one debug per axis, in the reference's order and wording.
    ///
    /// Asserts the FULL captured vector. A `.any()`/`.contains()` assertion here
    /// would still pass if the port emitted the header twice, dropped every
    /// axis but one, or emitted at the wrong level.
    #[test]
    fn log_variation_axes_emits_the_header_and_one_record_per_axis() -> Result<(), Error> {
        let mut lib: Library = Library::init()?;
        let mut face: Face = Face::init(
            lib,
            embedded::VARIABLE,
            opts(size(
                14.0,
                super::super::DEFAULT_DPI,
                super::super::DEFAULT_DPI,
            )),
        )?;

        let cap = crate::log_capture::start();
        face.log_variation_axes(true)?;
        let messages = cap.messages();
        drop(cap);

        assert_eq!(
            vec![
                (
                    log::Level::Debug,
                    "font_face".to_string(),
                    "variation axes font=JetBrains Mono".to_string()
                ),
                (
                    log::Level::Debug,
                    "font_face".to_string(),
                    "variation axis: name=Weight id=wght min=100 max=800 def=400".to_string()
                ),
            ],
            messages
        );

        face.deinit();
        lib.deinit();
        Ok(())
    }
}
