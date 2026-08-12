//! Port of the reference `harfbuzz` package
//! (`pkg/harfbuzz/{main,blob,buffer,common,face,font,freetype,coretext,shape,version,errors}.zig`)
//! — a hand-written Zig wrapper over the HarfBuzz C API, here implemented as
//! `unsafe` wrappers over `harfbuzz-sys`.
//!
//! Everything `main.zig:10-27` re-exports is carved: [`MemoryMode`]/[`Blob`],
//! [`Buffer`] (+ [`ContentType`]/[`ClusterLevel`]/[`GlyphInfo`]/[`GlyphPosition`]),
//! [`Direction`], [`Script`], [`Language`], [`Feature`] (+ [`FeatureExt`]),
//! [`Face`], [`Font`], [`shape`], [`Version`]/[`version`]/[`version_at_least`]/
//! [`version_string`], and the FreeType ([`create_face`]/[`create_font`]/
//! [`set_font_funcs`]) + CoreText ([`create_font_coretext`], macOS) bridges.
//! The package's 6 shared `test { ... }` blocks are ported below (`blob.zig:115`,
//! `buffer.zig:340`, `common.zig:244`, `freetype.zig:49`, `version.zig:34`, and
//! `main.zig:29`'s `refAllDecls`); `coretext.zig:18` is macOS-only and is gated
//! the same way there.
//!
//! PORT NOTE (`c`): the reference re-exports the raw C namespace as `harfbuzz.c`;
//! rustty's equivalent is the `harfbuzz_sys` crate itself, which callers import
//! directly.

use harfbuzz_sys as c;

/// HarfBuzz error.
// PORT NOTE: stands in for `pkg/harfbuzz/errors.zig`'s `Error.HarfbuzzFailed`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HarfbuzzFailed;

/// A HarfBuzz font object.
// PORT NOTE: `pkg/harfbuzz/font.zig`'s `Font`.
#[derive(Clone, Copy, Debug)]
pub struct Font {
    pub handle: *mut c::hb_font_t,
}

impl Font {
    /// Constructs a new font object from the specified face.
    // PORT NOTE: `Font.create`.
    pub fn create(face: Face) -> Result<Font, HarfbuzzFailed> {
        let handle = unsafe { c::hb_font_create(face.handle) };
        if handle.is_null() {
            Err(HarfbuzzFailed)
        } else {
            Ok(Font { handle })
        }
    }

    /// Decreases the reference count on the given font object, destroying it at
    /// zero and freeing all memory.
    // PORT NOTE: `Font.destroy`.
    pub fn destroy(self) {
        unsafe {
            c::hb_font_destroy(self.handle);
        }
    }

    // PORT NOTE: `pkg/harfbuzz/font.zig`'s `Font.setScale`.
    // pub fn setScale(self: *Font, x: u32, y: u32) void {
    //     c.hb_font_set_scale(self.handle, @intCast(x), @intCast(y));
    // }
    pub fn set_scale(self, x: u32, y: u32) {
        unsafe {
            c::hb_font_set_scale(self.handle, x as i32, y as i32);
        }
    }
}

/// Creates an `hb_font_t` font object from the specified `CTFontRef`.
///
// PORT NOTE: `pkg/harfbuzz/coretext.zig`'s `createFont` (the `hb_coretext_font_create`
// bridge, enabled by `HAVE_CORETEXT` in the vendored HarfBuzz build). `ct_font`
// is a type-erased `CTFontRef` (the caller passes its `objc2` `CTFont` pointer);
// it is cast to the bindgen `CTFontRef` (same underlying `CTFontRef`), mirroring
// the reference's custom extern that avoids a ptrcast mismatch.
#[cfg(target_os = "macos")]
pub fn create_font_coretext(ct_font: *const core::ffi::c_void) -> Result<Font, HarfbuzzFailed> {
    let handle = unsafe { c::hb_coretext_font_create(ct_font as c::CTFontRef) };
    if handle.is_null() {
        Err(HarfbuzzFailed)
    } else {
        Ok(Font { handle })
    }
}

/// The starting cluster of a global feature (inclusive).
// PORT NOTE: `harfbuzz.c.HB_FEATURE_GLOBAL_START`.
pub const HB_FEATURE_GLOBAL_START: u32 = c::HB_FEATURE_GLOBAL_START;

/// The ending cluster of a global feature (exclusive).
// PORT NOTE: `harfbuzz.c.HB_FEATURE_GLOBAL_END`. HarfBuzz defines this as
// `((unsigned int) -1)`; bindgen cannot const-evaluate that macro so it is
// reproduced here as `u32::MAX` (the identical bit pattern).
pub const HB_FEATURE_GLOBAL_END: u32 = u32::MAX;

/// Represents an OpenType feature setting passed to `shape`.
// PORT NOTE: `pkg/harfbuzz/common.zig`'s `Feature` (`extern struct`). The bindgen
// `hb_feature_t` has the identical layout (`tag`/`value`/`start`/`end`), so it
// is re-exported directly rather than re-declared.
pub type Feature = c::hb_feature_t;

/// The `hb_glyph_info_t` result of shaping (glyph index + source cluster).
// PORT NOTE: `pkg/harfbuzz/common.zig`'s `GlyphInfo` (`extern struct`). The shaper
// reads `.codepoint` (glyph index after shaping) and `.cluster`.
pub type GlyphInfo = c::hb_glyph_info_t;

/// The `hb_glyph_position_t` result of shaping (advances + offsets).
// PORT NOTE: `pkg/harfbuzz/common.zig`'s `GlyphPosition` (`extern struct`). The
// shaper reads `.x_advance`/`.y_advance`/`.x_offset`/`.y_offset`.
pub type GlyphPosition = c::hb_glyph_position_t;

/// The type of buffer contents.
// PORT NOTE: `pkg/harfbuzz/buffer.zig`'s `ContentType` (`enum(u2)`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContentType {
    /// Initial value for new buffer.
    #[allow(dead_code)]
    Invalid,
    /// The buffer contains input characters (before shaping).
    Unicode,
    /// The buffer contains output glyphs (after shaping).
    #[allow(dead_code)]
    Glyphs,
}

impl ContentType {
    fn to_c(self) -> c::hb_buffer_content_type_t {
        match self {
            ContentType::Invalid => c::hb_buffer_content_type_t_HB_BUFFER_CONTENT_TYPE_INVALID,
            ContentType::Unicode => c::hb_buffer_content_type_t_HB_BUFFER_CONTENT_TYPE_UNICODE,
            ContentType::Glyphs => c::hb_buffer_content_type_t_HB_BUFFER_CONTENT_TYPE_GLYPHS,
        }
    }

    /// `@enumFromInt(...)` — the inverse of `to_c`.
    // PORT NOTE: an unrepresentable tag decodes to `.invalid`, HarfBuzz's own
    // "initial value for a new buffer" sentinel (the reference `@enumFromInt`
    // cannot express that case at all — it is illegal behavior there).
    fn from_c(v: c::hb_buffer_content_type_t) -> ContentType {
        match v {
            c::hb_buffer_content_type_t_HB_BUFFER_CONTENT_TYPE_UNICODE => ContentType::Unicode,
            c::hb_buffer_content_type_t_HB_BUFFER_CONTENT_TYPE_GLYPHS => ContentType::Glyphs,
            _ => ContentType::Invalid,
        }
    }
}

/// HarfBuzz clustering behavior options.
// PORT NOTE: `pkg/harfbuzz/buffer.zig`'s `ClusterLevel` (`enum(u2)`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClusterLevel {
    /// Non-base characters merged into the preceding base cluster (default).
    #[allow(dead_code)]
    MonotoneGraphemes,
    /// Non-base characters get their own cluster; output stays monotone.
    #[allow(dead_code)]
    MonotoneCharacters,
    /// Non-base characters get their own cluster; not forced monotone.
    Characters,
    /// Grapheme clustering without forcing monotone order.
    #[allow(dead_code)]
    Graphemes,
}

impl ClusterLevel {
    fn to_c(self) -> c::hb_buffer_cluster_level_t {
        match self {
            ClusterLevel::MonotoneGraphemes => {
                c::hb_buffer_cluster_level_t_HB_BUFFER_CLUSTER_LEVEL_MONOTONE_GRAPHEMES
            }
            ClusterLevel::MonotoneCharacters => {
                c::hb_buffer_cluster_level_t_HB_BUFFER_CLUSTER_LEVEL_MONOTONE_CHARACTERS
            }
            ClusterLevel::Characters => {
                c::hb_buffer_cluster_level_t_HB_BUFFER_CLUSTER_LEVEL_CHARACTERS
            }
            ClusterLevel::Graphemes => {
                c::hb_buffer_cluster_level_t_HB_BUFFER_CLUSTER_LEVEL_GRAPHEMES
            }
        }
    }
}

/// The text flow direction of a buffer.
// PORT NOTE: `pkg/harfbuzz/common.zig`'s `Direction` (`enum(u3)`). Only the variants
// the shaper touches are exercised; the rest round out the enum for fidelity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    #[allow(dead_code)]
    Invalid,
    Ltr,
    #[allow(dead_code)]
    Rtl,
    #[allow(dead_code)]
    Ttb,
    #[allow(dead_code)]
    Btt,
}

impl Direction {
    fn to_c(self) -> c::hb_direction_t {
        match self {
            Direction::Invalid => c::hb_direction_t_HB_DIRECTION_INVALID,
            Direction::Ltr => c::hb_direction_t_HB_DIRECTION_LTR,
            Direction::Rtl => c::hb_direction_t_HB_DIRECTION_RTL,
            Direction::Ttb => c::hb_direction_t_HB_DIRECTION_TTB,
            Direction::Btt => c::hb_direction_t_HB_DIRECTION_BTT,
        }
    }

    /// `@enumFromInt(...)` — the inverse of `to_c`.
    // PORT NOTE: an unrepresentable tag decodes to `.invalid`, HarfBuzz's own
    // "no direction set" sentinel.
    fn from_c(v: c::hb_direction_t) -> Direction {
        match v {
            c::hb_direction_t_HB_DIRECTION_LTR => Direction::Ltr,
            c::hb_direction_t_HB_DIRECTION_RTL => Direction::Rtl,
            c::hb_direction_t_HB_DIRECTION_TTB => Direction::Ttb,
            c::hb_direction_t_HB_DIRECTION_BTT => Direction::Btt,
            _ => Direction::Invalid,
        }
    }
}

/// Buffers serve a dual role in HarfBuzz; before shaping, they hold the input
/// characters passed to `shape`, and after shaping they hold the output glyphs.
// PORT NOTE: `pkg/harfbuzz/buffer.zig`'s `Buffer` (only the surface the shaper uses).
#[derive(Clone, Copy, Debug)]
pub struct Buffer {
    pub handle: *mut c::hb_buffer_t,
}

impl Buffer {
    /// Creates a new `hb_buffer_t` with all properties at defaults.
    // pub fn create() Error!Buffer { ... }
    pub fn create() -> Result<Buffer, HarfbuzzFailed> {
        let handle = unsafe { c::hb_buffer_create() };
        if handle.is_null() {
            Err(HarfbuzzFailed)
        } else {
            Ok(Buffer { handle })
        }
    }

    /// Deallocate the buffer. Decreases its reference count by one; at zero the
    /// buffer and all associated resources are freed.
    // pub fn destroy(self: *Buffer) void { c.hb_buffer_destroy(self.handle); }
    pub fn destroy(self) {
        unsafe {
            c::hb_buffer_destroy(self.handle);
        }
    }

    /// Resets the buffer to its initial status.
    // pub fn reset(self: Buffer) void { c.hb_buffer_reset(self.handle); }
    pub fn reset(self) {
        unsafe {
            c::hb_buffer_reset(self.handle);
        }
    }

    /// Returns the number of items in the buffer.
    // pub fn getLength(self: Buffer) u32 { return c.hb_buffer_get_length(self.handle); }
    pub fn get_length(self) -> u32 {
        unsafe { c::hb_buffer_get_length(self.handle) }
    }

    /// Sets the type of buffer contents.
    // pub fn setContentType(self, ct) void { c.hb_buffer_set_content_type(...); }
    pub fn set_content_type(self, ct: ContentType) {
        unsafe {
            c::hb_buffer_set_content_type(self.handle, ct.to_c());
        }
    }

    /// Fetches the type of buffer contents.
    // pub fn getContentType(self: Buffer) ContentType { ... }
    pub fn get_content_type(self) -> ContentType {
        ContentType::from_c(unsafe { c::hb_buffer_get_content_type(self.handle) })
    }

    /// Appends a character with Unicode value `cp` and cluster value `cluster`.
    // pub fn add(self, cp: u32, cluster: u32) void { c.hb_buffer_add(...); }
    pub fn add(self, cp: u32, cluster: u32) {
        unsafe {
            c::hb_buffer_add(self.handle, cp, cluster);
        }
    }

    /// Appends characters from `text` to the buffer. The whole slice is both the
    /// context and the appended run (`item_offset = 0`, `item_length = len`).
    // pub fn addCodepoints(self: Buffer, text: []const u32) void { ... }
    pub fn add_codepoints(self, text: &[u32]) {
        unsafe {
            c::hb_buffer_add_codepoints(
                self.handle,
                text.as_ptr(),
                text.len() as core::ffi::c_int,
                0,
                text.len() as core::ffi::c_int,
            );
        }
    }

    /// See [`Buffer::add_codepoints`]. Replaces invalid UTF-32 characters with
    /// the buffer replacement code point.
    // pub fn addUTF32(self: Buffer, text: []const u32) void { ... }
    pub fn add_utf32(self, text: &[u32]) {
        unsafe {
            c::hb_buffer_add_utf32(
                self.handle,
                text.as_ptr(),
                text.len() as core::ffi::c_int,
                0,
                text.len() as core::ffi::c_int,
            );
        }
    }

    /// See [`Buffer::add_codepoints`]. Replaces invalid UTF-16 characters with
    /// the buffer replacement code point.
    // pub fn addUTF16(self: Buffer, text: []const u16) void { ... }
    pub fn add_utf16(self, text: &[u16]) {
        unsafe {
            c::hb_buffer_add_utf16(
                self.handle,
                text.as_ptr(),
                text.len() as core::ffi::c_int,
                0,
                text.len() as core::ffi::c_int,
            );
        }
    }

    /// See [`Buffer::add_codepoints`]. Replaces invalid UTF-8 characters with
    /// the buffer replacement code point.
    // pub fn addUTF8(self: Buffer, text: []const u8) void { ... }
    //
    // PORT NOTE: the reference takes `[]const u8` — a BYTE slice, not a validated
    // `str`; HarfBuzz itself substitutes the replacement codepoint for invalid
    // sequences, so the Rust signature keeps `&[u8]` rather than narrowing to `&str`.
    pub fn add_utf8(self, text: &[u8]) {
        unsafe {
            c::hb_buffer_add_utf8(
                self.handle,
                text.as_ptr().cast::<core::ffi::c_char>(),
                text.len() as core::ffi::c_int,
                0,
                text.len() as core::ffi::c_int,
            );
        }
    }

    /// Similar to [`Buffer::add_codepoints`], but allows only access to the first
    /// 256 Unicode code points that can fit in 8-bit strings.
    // pub fn addLatin1(self: Buffer, text: []const u8) void { ... }
    pub fn add_latin1(self, text: &[u8]) {
        unsafe {
            c::hb_buffer_add_latin1(
                self.handle,
                text.as_ptr(),
                text.len() as core::ffi::c_int,
                0,
                text.len() as core::ffi::c_int,
            );
        }
    }

    /// Set the text flow direction of the buffer.
    // pub fn setDirection(self, dir) void { c.hb_buffer_set_direction(...); }
    pub fn set_direction(self, dir: Direction) {
        unsafe {
            c::hb_buffer_set_direction(self.handle, dir.to_c());
        }
    }

    /// See [`Buffer::set_direction`].
    // pub fn getDirection(self: Buffer) Direction { ... }
    pub fn get_direction(self) -> Direction {
        Direction::from_c(unsafe { c::hb_buffer_get_direction(self.handle) })
    }

    /// Sets the script of the buffer to `script`.
    // pub fn setScript(self: Buffer, script: Script) void { ... }
    pub fn set_script(self, script: Script) {
        unsafe {
            c::hb_buffer_set_script(self.handle, script.to_c());
        }
    }

    /// See [`Buffer::set_script`].
    // pub fn getScript(self: Buffer) Script { ... }
    pub fn get_script(self) -> Script {
        Script::from_c(unsafe { c::hb_buffer_get_script(self.handle) })
    }

    /// Sets the language of the buffer to `language`.
    // pub fn setLanguage(self: Buffer, language: Language) void { ... }
    pub fn set_language(self, language: Language) {
        unsafe {
            c::hb_buffer_set_language(self.handle, language.handle);
        }
    }

    /// See [`Buffer::set_language`].
    // pub fn getLanguage(self: Buffer) Language { ... }
    pub fn get_language(self) -> Language {
        Language {
            handle: unsafe { c::hb_buffer_get_language(self.handle) },
        }
    }

    /// Sets unset buffer segment properties from the buffer's Unicode contents.
    // pub fn guessSegmentProperties(self) void { c.hb_buffer_guess_segment_properties(...); }
    pub fn guess_segment_properties(self) {
        unsafe {
            c::hb_buffer_guess_segment_properties(self.handle);
        }
    }

    /// Sets the cluster level of a buffer.
    // pub fn setClusterLevel(self, level) void { c.hb_buffer_set_cluster_level(...); }
    pub fn set_cluster_level(self, level: ClusterLevel) {
        unsafe {
            c::hb_buffer_set_cluster_level(self.handle, level.to_c());
        }
    }

    /// Returns the buffer glyph information array. Valid as long as buffer
    /// contents are not modified.
    // pub fn getGlyphInfos(self: Buffer) []GlyphInfo { ... }
    pub fn get_glyph_infos(self) -> &'static [GlyphInfo] {
        let mut length: u32 = 0;
        let ptr = unsafe { c::hb_buffer_get_glyph_infos(self.handle, &mut length) };
        if ptr.is_null() {
            return &[];
        }
        unsafe { core::slice::from_raw_parts(ptr, length as usize) }
    }

    /// Returns the buffer glyph position array, or `None` (e.g. within a buffer
    /// message callback). Valid as long as buffer contents are not modified.
    // pub fn getGlyphPositions(self: Buffer) ?[]GlyphPosition { ... }
    pub fn get_glyph_positions(self) -> Option<&'static [GlyphPosition]> {
        let mut length: u32 = 0;
        let ptr = unsafe { c::hb_buffer_get_glyph_positions(self.handle, &mut length) };
        if ptr.is_null() {
            None
        } else {
            Some(unsafe { core::slice::from_raw_parts(ptr, length as usize) })
        }
    }
}

/// Shapes `buf` using `font`, turning its Unicode content into positioned
/// glyphs. If `features` is non-empty it controls the applied features.
// PORT NOTE: `pkg/harfbuzz/shape.zig`'s `shape`.
// pub fn shape(font: Font, buf: Buffer, features: ?[]const Feature) void { ... }
pub fn shape(font: Font, buf: Buffer, features: Option<&[Feature]>) {
    // const hb_feats: [*c]const c.hb_feature_t = feats: {
    //     if (features) |fs| if (fs.len > 0) break :feats @ptrCast(fs.ptr);
    //     break :feats null;
    // };
    let hb_feats: *const c::hb_feature_t = match features {
        Some(fs) if !fs.is_empty() => fs.as_ptr(),
        _ => core::ptr::null(),
    };

    unsafe {
        c::hb_shape(
            font.handle,
            buf.handle,
            hb_feats,
            match features {
                Some(f) => f.len() as core::ffi::c_uint,
                None => 0,
            },
        );
    }
}

/// Creates an `hb_font_t` font object from the specified `FT_Face`.
///
/// Calls `FT_Reference_Face` on `ft_face`, ensuring it remains alive as long as
/// the resulting font object does, and `FT_Done_Face` when the font is destroyed.
// PORT NOTE: `pkg/harfbuzz/freetype.zig`'s `createFont` (via the custom extern
// `hb_ft_font_create_referenced` so the correct FreeType struct is used).
pub fn create_font(face: freetype_sys::FT_Face) -> Result<Font, HarfbuzzFailed> {
    // `harfbuzz-sys` binds its own `FT_Face` type; cast the `freetype-sys`
    // pointer to it (same underlying C `FT_Face`), mirroring the reference's
    // custom-extern trick that avoids a ptrcast mismatch.
    let handle = unsafe { c::hb_ft_font_create_referenced(face.cast()) };
    if handle.is_null() {
        Err(HarfbuzzFailed)
    } else {
        Ok(Font { handle })
    }
}

/// Creates an `hb_face_t` face object from the specified `FT_Face`.
///
/// This is the preferred variant of the `hb_ft_face_create*` function family,
/// because it calls `FT_Reference_Face` on `ft_face`, ensuring that `ft_face`
/// remains alive as long as the resulting `hb_face_t` face object remains alive.
/// Also calls `FT_Done_Face` when the `hb_face_t` face object is destroyed.
///
/// Use this version unless you know you have good reasons not to.
// PORT NOTE: `pkg/harfbuzz/freetype.zig`'s `createFace` (via the custom extern
// `hb_ft_face_create_referenced`, same `FT_Face` cast rationale as `create_font`).
pub fn create_face(face: freetype_sys::FT_Face) -> Result<Face, HarfbuzzFailed> {
    let handle = unsafe { c::hb_ft_face_create_referenced(face.cast()) };
    if handle.is_null() {
        Err(HarfbuzzFailed)
    } else {
        Ok(Face { handle })
    }
}

/// Configures the font-functions structure of the specified `hb_font_t` font
/// object to use FreeType font functions.
///
/// In particular, you can use this function to configure an existing `hb_face_t`
/// face object for use with FreeType font functions even if that `hb_face_t` face
/// object was initially created with `hb_face_create()`, and therefore was not
/// initially configured to use FreeType font functions.
///
/// An `hb_face_t` face object created with `hb_ft_face_create()` is preconfigured
/// for FreeType font functions and does not require this function to be used.
// PORT NOTE: `pkg/harfbuzz/freetype.zig`'s `setFontFuncs`.
pub fn set_font_funcs(font: Font) {
    unsafe {
        c::hb_ft_font_set_funcs(font.handle);
    }
}

// ---------------------------------------------------------------------------
// `pkg/harfbuzz/version.zig`

/// The HarfBuzz library version triple.
// PORT NOTE: `version.zig`'s `Version`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub micro: u32,
}

/// Returns library version as three integer components.
// PORT NOTE: `version.zig`'s `version`.
pub fn version() -> Version {
    let mut major: core::ffi::c_uint = 0;
    let mut minor: core::ffi::c_uint = 0;
    let mut micro: core::ffi::c_uint = 0;
    unsafe {
        c::hb_version(&mut major, &mut minor, &mut micro);
    }
    Version {
        major,
        minor,
        micro,
    }
}

/// Tests the library version against a minimum value, as three integer components.
// PORT NOTE: `version.zig`'s `versionAtLeast`.
pub fn version_at_least(vsn: Version) -> bool {
    unsafe { c::hb_version_atleast(vsn.major, vsn.minor, vsn.micro) > 0 }
}

/// Returns library version as a string with three components.
// PORT NOTE: `version.zig`'s `versionString`. The reference returns the
// NUL-terminated `[:0]const u8` slice; rustty returns the borrowed `CStr` (its
// bytes are a static HarfBuzz literal, never a lossy `String`).
pub fn version_string() -> &'static core::ffi::CStr {
    unsafe { core::ffi::CStr::from_ptr(c::hb_version_string()) }
}

// ---------------------------------------------------------------------------
// `pkg/harfbuzz/blob.zig`

/// Data type holding the memory modes available to client programs.
///
/// Regarding these various memory-modes:
///
///   - In no case shall the HarfBuzz client modify memory that is passed to
///     HarfBuzz in a blob. If there is any such possibility,
///     HB_MEMORY_MODE_DUPLICATE should be used such that HarfBuzz makes a
///     copy immediately,
///
///   - Use HB_MEMORY_MODE_READONLY otherwise, unless you really really really
///     know what you are doing,
///
///   - HB_MEMORY_MODE_WRITABLE is appropriate if you really made a copy of
///     data solely for the purpose of passing to HarfBuzz and doing that
///     just once (no reuse!),
///
///   - If the font is mmap()ed, it's okay to use
///     HB_MEMORY_READONLY_MAY_MAKE_WRITABLE , however, using that mode
///     correctly is very tricky. Use HB_MEMORY_MODE_READONLY instead.
// PORT NOTE: `blob.zig`'s `MemoryMode = enum(u2)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemoryMode {
    /// HarfBuzz immediately makes a copy of the data.
    Duplicate,
    /// HarfBuzz client will never modify the data, and HarfBuzz will never
    /// modify the data.
    Readonly,
    /// HarfBuzz client made a copy of the data solely for HarfBuzz, so
    /// HarfBuzz may modify the data.
    Writable,
    /// See above
    ReadonlyMayMakeWritable,
}

impl MemoryMode {
    /// `@intFromEnum(mode)` — the raw `hb_memory_mode_t` value.
    pub fn to_c(self) -> c::hb_memory_mode_t {
        match self {
            MemoryMode::Duplicate => c::hb_memory_mode_t_HB_MEMORY_MODE_DUPLICATE,
            MemoryMode::Readonly => c::hb_memory_mode_t_HB_MEMORY_MODE_READONLY,
            MemoryMode::Writable => c::hb_memory_mode_t_HB_MEMORY_MODE_WRITABLE,
            MemoryMode::ReadonlyMayMakeWritable => {
                c::hb_memory_mode_t_HB_MEMORY_MODE_READONLY_MAY_MAKE_WRITABLE
            }
        }
    }
}

/// Blobs wrap a chunk of binary data to handle lifecycle management of data
/// while it is passed between client and HarfBuzz. Blobs are primarily
/// used to create font faces, but also to access font face tables, as well as
/// pass around other binary data.
// PORT NOTE: `blob.zig`'s `Blob`.
#[derive(Clone, Copy, Debug)]
pub struct Blob {
    pub handle: *mut c::hb_blob_t,
}

impl Blob {
    /// Creates a new "blob" object wrapping data . The mode parameter is used
    /// to negotiate ownership and lifecycle of data .
    ///
    /// Note that this function returns a freshly-allocated empty blob even
    /// if length is zero. This is in contrast to hb_blob_create(), which
    /// returns the singleton empty blob (as returned by hb_blob_get_empty())
    /// if length is zero.
    // PORT NOTE: `Blob.create`. `data` is a BYTE slice (never a lossy `String`).
    pub fn create(data: &[u8], mode: MemoryMode) -> Result<Blob, HarfbuzzFailed> {
        let handle = unsafe {
            c::hb_blob_create_or_fail(
                data.as_ptr().cast::<core::ffi::c_char>(),
                data.len() as core::ffi::c_uint,
                mode.to_c(),
                core::ptr::null_mut(),
                None,
            )
        };
        if handle.is_null() {
            Err(HarfbuzzFailed)
        } else {
            Ok(Blob { handle })
        }
    }

    /// Decreases the reference count on blob , and if it reaches zero,
    /// destroys blob , freeing all memory, possibly calling the
    /// destroy-callback the blob was created for if it has not been
    /// called already.
    // PORT NOTE: `Blob.destroy`.
    pub fn destroy(self) {
        unsafe {
            c::hb_blob_destroy(self.handle);
        }
    }

    /// Attaches a user-data key/data pair to the specified blob.
    ///
    /// # Safety
    /// `key` must outlive every `get_user_data` that reads it (HarfBuzz keys the
    /// entry on the pointer's ADDRESS), and `ptr` must stay valid for as long as
    /// the blob holds it — or `destroycb` must free it.
    // PORT NOTE: `Blob.setUserData`. The reference's `comptime destroycb` is
    // wrapped in a comptime trampoline that ptr-casts `?*anyopaque` back to `?*T`;
    // a thin `*mut T` and `*mut c_void` share an identical C ABI, so the Rust
    // equivalent is a transmute of the function pointer — no trampoline frame.
    pub unsafe fn set_user_data<T>(
        self,
        key: *mut c::hb_user_data_key_t,
        ptr: *mut T,
        destroycb: Option<unsafe extern "C" fn(*mut T)>,
        replace: bool,
    ) -> bool {
        let cb: c::hb_destroy_func_t = destroycb.map(|f| unsafe {
            core::mem::transmute::<
                unsafe extern "C" fn(*mut T),
                unsafe extern "C" fn(*mut core::ffi::c_void),
            >(f)
        });
        unsafe {
            c::hb_blob_set_user_data(
                self.handle,
                key,
                ptr.cast::<core::ffi::c_void>(),
                cb,
                if replace { 1 } else { 0 },
            ) > 0
        }
    }

    /// Fetches the user data associated with the specified key, attached to
    /// the specified font-functions structure.
    ///
    /// # Safety
    /// `T` must be the type the matching [`Blob::set_user_data`] stored.
    // PORT NOTE: `Blob.getUserData`.
    pub unsafe fn get_user_data<T>(self, key: *mut c::hb_user_data_key_t) -> Option<*mut T> {
        let opt = unsafe { c::hb_blob_get_user_data(self.handle, key) };
        if opt.is_null() {
            None
        } else {
            Some(opt.cast::<T>())
        }
    }
}

// ---------------------------------------------------------------------------
// `pkg/harfbuzz/face.zig`

/// A font face is an object that represents a single face from within a font family.
///
/// More precisely, a font face represents a single face in a binary font file.
/// Font faces are typically built from a binary blob and a face index.
/// Font faces are used to create fonts.
// PORT NOTE: `face.zig`'s `Face`.
#[derive(Clone, Copy, Debug)]
pub struct Face {
    pub handle: *mut c::hb_face_t,
}

impl Face {
    /// Decreases the reference count on a face object. When the reference
    /// count reaches zero, the face is destroyed, freeing all memory.
    // PORT NOTE: `Face.destroy`.
    pub fn destroy(self) {
        unsafe {
            c::hb_face_destroy(self.handle);
        }
    }
}

// ---------------------------------------------------------------------------
// `pkg/harfbuzz/common.zig` — `Script` / `Language` / `Feature`

/// Data type for scripts. Each `hb_script_t`'s value is an `hb_tag_t`
/// corresponding to the four-letter values defined by ISO 15924.
///
/// See also the Script (sc) property of the Unicode Character Database.
// PORT NOTE: `pkg/harfbuzz/common.zig`'s `Script = enum(u31)` — all 164 variants,
// in the reference's declaration order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Script {
    Common,
    Inherited,
    Unknown,
    Arabic,
    Armenian,
    Bengali,
    Cyrillic,
    Devanagari,
    Georgian,
    Greek,
    Gujarati,
    Gurmukhi,
    Hangul,
    Han,
    Hebrew,
    Hiragana,
    Kannada,
    Katakana,
    Lao,
    Latin,
    Malayalam,
    Oriya,
    Tamil,
    Telugu,
    Thai,
    Tibetan,
    Bopomofo,
    Braille,
    CanadianSyllabics,
    Cherokee,
    Ethiopic,
    Khmer,
    Mongolian,
    Myanmar,
    Ogham,
    Runic,
    Sinhala,
    Syriac,
    Thaana,
    Yi,
    Deseret,
    Gothic,
    OldItalic,
    Buhid,
    Hanunoo,
    Tagalog,
    Tagbanwa,
    Cypriot,
    Limbu,
    LinearB,
    Osmanya,
    Shavian,
    TaiLe,
    Ugaritic,
    Buginese,
    Coptic,
    Glagolitic,
    Kharoshthi,
    NewTaiLue,
    OldPersian,
    SylotiNagri,
    Tifinagh,
    Balinese,
    Cuneiform,
    Nko,
    PhagsPa,
    Phoenician,
    Carian,
    Cham,
    KayahLi,
    Lepcha,
    Lycian,
    Lydian,
    OlChiki,
    Rejang,
    Saurashtra,
    Sundanese,
    Vai,
    Avestan,
    Bamum,
    EgyptianHieroglyphs,
    ImperialAramaic,
    InscriptionalPahlavi,
    InscriptionalParthian,
    Javanese,
    Kaithi,
    Lisu,
    MeeteiMayek,
    OldSouthArabian,
    OldTurkic,
    Samaritan,
    TaiTham,
    TaiViet,
    Batak,
    Brahmi,
    Mandaic,
    Chakma,
    MeroiticCursive,
    MeroiticHieroglyphs,
    Miao,
    Sharada,
    SoraSompeng,
    Takri,
    BassaVah,
    CaucasianAlbanian,
    Duployan,
    Elbasan,
    Grantha,
    Khojki,
    Khudawadi,
    LinearA,
    Mahajani,
    Manichaean,
    MendeKikakui,
    Modi,
    Mro,
    Nabataean,
    OldNorthArabian,
    OldPermic,
    PahawhHmong,
    Palmyrene,
    PauCinHau,
    PsalterPahlavi,
    Siddham,
    Tirhuta,
    WarangCiti,
    Ahom,
    AnatolianHieroglyphs,
    Hatran,
    Multani,
    OldHungarian,
    Signwriting,
    Adlam,
    Bhaiksuki,
    Marchen,
    Osage,
    Tangut,
    Newa,
    MasaramGondi,
    Nushu,
    Soyombo,
    ZanabazarSquare,
    Dogra,
    GunjalaGondi,
    HanifiRohingya,
    Makasar,
    Medefaidrin,
    OldSogdian,
    Sogdian,
    Elymaic,
    Nandinagari,
    NyiakengPuachueHmong,
    Wancho,
    Chorasmian,
    DivesAkuru,
    KhitanSmallScript,
    Yezidi,
    CyproMinoan,
    OldUyghur,
    Tangsa,
    Toto,
    Vithkuqi,
    Math,
    Invalid,
}

impl Script {
    /// `@intFromEnum(script)` — the raw `hb_script_t` tag value.
    pub fn to_c(self) -> c::hb_script_t {
        match self {
            Script::Common => c::hb_script_t_HB_SCRIPT_COMMON,
            Script::Inherited => c::hb_script_t_HB_SCRIPT_INHERITED,
            Script::Unknown => c::hb_script_t_HB_SCRIPT_UNKNOWN,
            Script::Arabic => c::hb_script_t_HB_SCRIPT_ARABIC,
            Script::Armenian => c::hb_script_t_HB_SCRIPT_ARMENIAN,
            Script::Bengali => c::hb_script_t_HB_SCRIPT_BENGALI,
            Script::Cyrillic => c::hb_script_t_HB_SCRIPT_CYRILLIC,
            Script::Devanagari => c::hb_script_t_HB_SCRIPT_DEVANAGARI,
            Script::Georgian => c::hb_script_t_HB_SCRIPT_GEORGIAN,
            Script::Greek => c::hb_script_t_HB_SCRIPT_GREEK,
            Script::Gujarati => c::hb_script_t_HB_SCRIPT_GUJARATI,
            Script::Gurmukhi => c::hb_script_t_HB_SCRIPT_GURMUKHI,
            Script::Hangul => c::hb_script_t_HB_SCRIPT_HANGUL,
            Script::Han => c::hb_script_t_HB_SCRIPT_HAN,
            Script::Hebrew => c::hb_script_t_HB_SCRIPT_HEBREW,
            Script::Hiragana => c::hb_script_t_HB_SCRIPT_HIRAGANA,
            Script::Kannada => c::hb_script_t_HB_SCRIPT_KANNADA,
            Script::Katakana => c::hb_script_t_HB_SCRIPT_KATAKANA,
            Script::Lao => c::hb_script_t_HB_SCRIPT_LAO,
            Script::Latin => c::hb_script_t_HB_SCRIPT_LATIN,
            Script::Malayalam => c::hb_script_t_HB_SCRIPT_MALAYALAM,
            Script::Oriya => c::hb_script_t_HB_SCRIPT_ORIYA,
            Script::Tamil => c::hb_script_t_HB_SCRIPT_TAMIL,
            Script::Telugu => c::hb_script_t_HB_SCRIPT_TELUGU,
            Script::Thai => c::hb_script_t_HB_SCRIPT_THAI,
            Script::Tibetan => c::hb_script_t_HB_SCRIPT_TIBETAN,
            Script::Bopomofo => c::hb_script_t_HB_SCRIPT_BOPOMOFO,
            Script::Braille => c::hb_script_t_HB_SCRIPT_BRAILLE,
            Script::CanadianSyllabics => c::hb_script_t_HB_SCRIPT_CANADIAN_SYLLABICS,
            Script::Cherokee => c::hb_script_t_HB_SCRIPT_CHEROKEE,
            Script::Ethiopic => c::hb_script_t_HB_SCRIPT_ETHIOPIC,
            Script::Khmer => c::hb_script_t_HB_SCRIPT_KHMER,
            Script::Mongolian => c::hb_script_t_HB_SCRIPT_MONGOLIAN,
            Script::Myanmar => c::hb_script_t_HB_SCRIPT_MYANMAR,
            Script::Ogham => c::hb_script_t_HB_SCRIPT_OGHAM,
            Script::Runic => c::hb_script_t_HB_SCRIPT_RUNIC,
            Script::Sinhala => c::hb_script_t_HB_SCRIPT_SINHALA,
            Script::Syriac => c::hb_script_t_HB_SCRIPT_SYRIAC,
            Script::Thaana => c::hb_script_t_HB_SCRIPT_THAANA,
            Script::Yi => c::hb_script_t_HB_SCRIPT_YI,
            Script::Deseret => c::hb_script_t_HB_SCRIPT_DESERET,
            Script::Gothic => c::hb_script_t_HB_SCRIPT_GOTHIC,
            Script::OldItalic => c::hb_script_t_HB_SCRIPT_OLD_ITALIC,
            Script::Buhid => c::hb_script_t_HB_SCRIPT_BUHID,
            Script::Hanunoo => c::hb_script_t_HB_SCRIPT_HANUNOO,
            Script::Tagalog => c::hb_script_t_HB_SCRIPT_TAGALOG,
            Script::Tagbanwa => c::hb_script_t_HB_SCRIPT_TAGBANWA,
            Script::Cypriot => c::hb_script_t_HB_SCRIPT_CYPRIOT,
            Script::Limbu => c::hb_script_t_HB_SCRIPT_LIMBU,
            Script::LinearB => c::hb_script_t_HB_SCRIPT_LINEAR_B,
            Script::Osmanya => c::hb_script_t_HB_SCRIPT_OSMANYA,
            Script::Shavian => c::hb_script_t_HB_SCRIPT_SHAVIAN,
            Script::TaiLe => c::hb_script_t_HB_SCRIPT_TAI_LE,
            Script::Ugaritic => c::hb_script_t_HB_SCRIPT_UGARITIC,
            Script::Buginese => c::hb_script_t_HB_SCRIPT_BUGINESE,
            Script::Coptic => c::hb_script_t_HB_SCRIPT_COPTIC,
            Script::Glagolitic => c::hb_script_t_HB_SCRIPT_GLAGOLITIC,
            Script::Kharoshthi => c::hb_script_t_HB_SCRIPT_KHAROSHTHI,
            Script::NewTaiLue => c::hb_script_t_HB_SCRIPT_NEW_TAI_LUE,
            Script::OldPersian => c::hb_script_t_HB_SCRIPT_OLD_PERSIAN,
            Script::SylotiNagri => c::hb_script_t_HB_SCRIPT_SYLOTI_NAGRI,
            Script::Tifinagh => c::hb_script_t_HB_SCRIPT_TIFINAGH,
            Script::Balinese => c::hb_script_t_HB_SCRIPT_BALINESE,
            Script::Cuneiform => c::hb_script_t_HB_SCRIPT_CUNEIFORM,
            Script::Nko => c::hb_script_t_HB_SCRIPT_NKO,
            Script::PhagsPa => c::hb_script_t_HB_SCRIPT_PHAGS_PA,
            Script::Phoenician => c::hb_script_t_HB_SCRIPT_PHOENICIAN,
            Script::Carian => c::hb_script_t_HB_SCRIPT_CARIAN,
            Script::Cham => c::hb_script_t_HB_SCRIPT_CHAM,
            Script::KayahLi => c::hb_script_t_HB_SCRIPT_KAYAH_LI,
            Script::Lepcha => c::hb_script_t_HB_SCRIPT_LEPCHA,
            Script::Lycian => c::hb_script_t_HB_SCRIPT_LYCIAN,
            Script::Lydian => c::hb_script_t_HB_SCRIPT_LYDIAN,
            Script::OlChiki => c::hb_script_t_HB_SCRIPT_OL_CHIKI,
            Script::Rejang => c::hb_script_t_HB_SCRIPT_REJANG,
            Script::Saurashtra => c::hb_script_t_HB_SCRIPT_SAURASHTRA,
            Script::Sundanese => c::hb_script_t_HB_SCRIPT_SUNDANESE,
            Script::Vai => c::hb_script_t_HB_SCRIPT_VAI,
            Script::Avestan => c::hb_script_t_HB_SCRIPT_AVESTAN,
            Script::Bamum => c::hb_script_t_HB_SCRIPT_BAMUM,
            Script::EgyptianHieroglyphs => c::hb_script_t_HB_SCRIPT_EGYPTIAN_HIEROGLYPHS,
            Script::ImperialAramaic => c::hb_script_t_HB_SCRIPT_IMPERIAL_ARAMAIC,
            Script::InscriptionalPahlavi => c::hb_script_t_HB_SCRIPT_INSCRIPTIONAL_PAHLAVI,
            Script::InscriptionalParthian => c::hb_script_t_HB_SCRIPT_INSCRIPTIONAL_PARTHIAN,
            Script::Javanese => c::hb_script_t_HB_SCRIPT_JAVANESE,
            Script::Kaithi => c::hb_script_t_HB_SCRIPT_KAITHI,
            Script::Lisu => c::hb_script_t_HB_SCRIPT_LISU,
            Script::MeeteiMayek => c::hb_script_t_HB_SCRIPT_MEETEI_MAYEK,
            Script::OldSouthArabian => c::hb_script_t_HB_SCRIPT_OLD_SOUTH_ARABIAN,
            Script::OldTurkic => c::hb_script_t_HB_SCRIPT_OLD_TURKIC,
            Script::Samaritan => c::hb_script_t_HB_SCRIPT_SAMARITAN,
            Script::TaiTham => c::hb_script_t_HB_SCRIPT_TAI_THAM,
            Script::TaiViet => c::hb_script_t_HB_SCRIPT_TAI_VIET,
            Script::Batak => c::hb_script_t_HB_SCRIPT_BATAK,
            Script::Brahmi => c::hb_script_t_HB_SCRIPT_BRAHMI,
            Script::Mandaic => c::hb_script_t_HB_SCRIPT_MANDAIC,
            Script::Chakma => c::hb_script_t_HB_SCRIPT_CHAKMA,
            Script::MeroiticCursive => c::hb_script_t_HB_SCRIPT_MEROITIC_CURSIVE,
            Script::MeroiticHieroglyphs => c::hb_script_t_HB_SCRIPT_MEROITIC_HIEROGLYPHS,
            Script::Miao => c::hb_script_t_HB_SCRIPT_MIAO,
            Script::Sharada => c::hb_script_t_HB_SCRIPT_SHARADA,
            Script::SoraSompeng => c::hb_script_t_HB_SCRIPT_SORA_SOMPENG,
            Script::Takri => c::hb_script_t_HB_SCRIPT_TAKRI,
            Script::BassaVah => c::hb_script_t_HB_SCRIPT_BASSA_VAH,
            Script::CaucasianAlbanian => c::hb_script_t_HB_SCRIPT_CAUCASIAN_ALBANIAN,
            Script::Duployan => c::hb_script_t_HB_SCRIPT_DUPLOYAN,
            Script::Elbasan => c::hb_script_t_HB_SCRIPT_ELBASAN,
            Script::Grantha => c::hb_script_t_HB_SCRIPT_GRANTHA,
            Script::Khojki => c::hb_script_t_HB_SCRIPT_KHOJKI,
            Script::Khudawadi => c::hb_script_t_HB_SCRIPT_KHUDAWADI,
            Script::LinearA => c::hb_script_t_HB_SCRIPT_LINEAR_A,
            Script::Mahajani => c::hb_script_t_HB_SCRIPT_MAHAJANI,
            Script::Manichaean => c::hb_script_t_HB_SCRIPT_MANICHAEAN,
            Script::MendeKikakui => c::hb_script_t_HB_SCRIPT_MENDE_KIKAKUI,
            Script::Modi => c::hb_script_t_HB_SCRIPT_MODI,
            Script::Mro => c::hb_script_t_HB_SCRIPT_MRO,
            Script::Nabataean => c::hb_script_t_HB_SCRIPT_NABATAEAN,
            Script::OldNorthArabian => c::hb_script_t_HB_SCRIPT_OLD_NORTH_ARABIAN,
            Script::OldPermic => c::hb_script_t_HB_SCRIPT_OLD_PERMIC,
            Script::PahawhHmong => c::hb_script_t_HB_SCRIPT_PAHAWH_HMONG,
            Script::Palmyrene => c::hb_script_t_HB_SCRIPT_PALMYRENE,
            Script::PauCinHau => c::hb_script_t_HB_SCRIPT_PAU_CIN_HAU,
            Script::PsalterPahlavi => c::hb_script_t_HB_SCRIPT_PSALTER_PAHLAVI,
            Script::Siddham => c::hb_script_t_HB_SCRIPT_SIDDHAM,
            Script::Tirhuta => c::hb_script_t_HB_SCRIPT_TIRHUTA,
            Script::WarangCiti => c::hb_script_t_HB_SCRIPT_WARANG_CITI,
            Script::Ahom => c::hb_script_t_HB_SCRIPT_AHOM,
            Script::AnatolianHieroglyphs => c::hb_script_t_HB_SCRIPT_ANATOLIAN_HIEROGLYPHS,
            Script::Hatran => c::hb_script_t_HB_SCRIPT_HATRAN,
            Script::Multani => c::hb_script_t_HB_SCRIPT_MULTANI,
            Script::OldHungarian => c::hb_script_t_HB_SCRIPT_OLD_HUNGARIAN,
            Script::Signwriting => c::hb_script_t_HB_SCRIPT_SIGNWRITING,
            Script::Adlam => c::hb_script_t_HB_SCRIPT_ADLAM,
            Script::Bhaiksuki => c::hb_script_t_HB_SCRIPT_BHAIKSUKI,
            Script::Marchen => c::hb_script_t_HB_SCRIPT_MARCHEN,
            Script::Osage => c::hb_script_t_HB_SCRIPT_OSAGE,
            Script::Tangut => c::hb_script_t_HB_SCRIPT_TANGUT,
            Script::Newa => c::hb_script_t_HB_SCRIPT_NEWA,
            Script::MasaramGondi => c::hb_script_t_HB_SCRIPT_MASARAM_GONDI,
            Script::Nushu => c::hb_script_t_HB_SCRIPT_NUSHU,
            Script::Soyombo => c::hb_script_t_HB_SCRIPT_SOYOMBO,
            Script::ZanabazarSquare => c::hb_script_t_HB_SCRIPT_ZANABAZAR_SQUARE,
            Script::Dogra => c::hb_script_t_HB_SCRIPT_DOGRA,
            Script::GunjalaGondi => c::hb_script_t_HB_SCRIPT_GUNJALA_GONDI,
            Script::HanifiRohingya => c::hb_script_t_HB_SCRIPT_HANIFI_ROHINGYA,
            Script::Makasar => c::hb_script_t_HB_SCRIPT_MAKASAR,
            Script::Medefaidrin => c::hb_script_t_HB_SCRIPT_MEDEFAIDRIN,
            Script::OldSogdian => c::hb_script_t_HB_SCRIPT_OLD_SOGDIAN,
            Script::Sogdian => c::hb_script_t_HB_SCRIPT_SOGDIAN,
            Script::Elymaic => c::hb_script_t_HB_SCRIPT_ELYMAIC,
            Script::Nandinagari => c::hb_script_t_HB_SCRIPT_NANDINAGARI,
            Script::NyiakengPuachueHmong => c::hb_script_t_HB_SCRIPT_NYIAKENG_PUACHUE_HMONG,
            Script::Wancho => c::hb_script_t_HB_SCRIPT_WANCHO,
            Script::Chorasmian => c::hb_script_t_HB_SCRIPT_CHORASMIAN,
            Script::DivesAkuru => c::hb_script_t_HB_SCRIPT_DIVES_AKURU,
            Script::KhitanSmallScript => c::hb_script_t_HB_SCRIPT_KHITAN_SMALL_SCRIPT,
            Script::Yezidi => c::hb_script_t_HB_SCRIPT_YEZIDI,
            Script::CyproMinoan => c::hb_script_t_HB_SCRIPT_CYPRO_MINOAN,
            Script::OldUyghur => c::hb_script_t_HB_SCRIPT_OLD_UYGHUR,
            Script::Tangsa => c::hb_script_t_HB_SCRIPT_TANGSA,
            Script::Toto => c::hb_script_t_HB_SCRIPT_TOTO,
            Script::Vithkuqi => c::hb_script_t_HB_SCRIPT_VITHKUQI,
            Script::Math => c::hb_script_t_HB_SCRIPT_MATH,
            Script::Invalid => c::hb_script_t_HB_SCRIPT_INVALID,
        }
    }

    /// `@enumFromInt(...)` — the inverse of [`Script::to_c`].
    //
    // PORT NOTE: the reference `@enumFromInt` is UB (safety-checked illegal
    // behavior) for a tag outside the enum; Rust cannot construct such a value at
    // all, so an unknown tag decodes to `Script::Invalid` — the HarfBuzz sentinel
    // (`HB_SCRIPT_INVALID`) for "no script set".
    pub fn from_c(v: c::hb_script_t) -> Script {
        match v {
            c::hb_script_t_HB_SCRIPT_COMMON => Script::Common,
            c::hb_script_t_HB_SCRIPT_INHERITED => Script::Inherited,
            c::hb_script_t_HB_SCRIPT_UNKNOWN => Script::Unknown,
            c::hb_script_t_HB_SCRIPT_ARABIC => Script::Arabic,
            c::hb_script_t_HB_SCRIPT_ARMENIAN => Script::Armenian,
            c::hb_script_t_HB_SCRIPT_BENGALI => Script::Bengali,
            c::hb_script_t_HB_SCRIPT_CYRILLIC => Script::Cyrillic,
            c::hb_script_t_HB_SCRIPT_DEVANAGARI => Script::Devanagari,
            c::hb_script_t_HB_SCRIPT_GEORGIAN => Script::Georgian,
            c::hb_script_t_HB_SCRIPT_GREEK => Script::Greek,
            c::hb_script_t_HB_SCRIPT_GUJARATI => Script::Gujarati,
            c::hb_script_t_HB_SCRIPT_GURMUKHI => Script::Gurmukhi,
            c::hb_script_t_HB_SCRIPT_HANGUL => Script::Hangul,
            c::hb_script_t_HB_SCRIPT_HAN => Script::Han,
            c::hb_script_t_HB_SCRIPT_HEBREW => Script::Hebrew,
            c::hb_script_t_HB_SCRIPT_HIRAGANA => Script::Hiragana,
            c::hb_script_t_HB_SCRIPT_KANNADA => Script::Kannada,
            c::hb_script_t_HB_SCRIPT_KATAKANA => Script::Katakana,
            c::hb_script_t_HB_SCRIPT_LAO => Script::Lao,
            c::hb_script_t_HB_SCRIPT_LATIN => Script::Latin,
            c::hb_script_t_HB_SCRIPT_MALAYALAM => Script::Malayalam,
            c::hb_script_t_HB_SCRIPT_ORIYA => Script::Oriya,
            c::hb_script_t_HB_SCRIPT_TAMIL => Script::Tamil,
            c::hb_script_t_HB_SCRIPT_TELUGU => Script::Telugu,
            c::hb_script_t_HB_SCRIPT_THAI => Script::Thai,
            c::hb_script_t_HB_SCRIPT_TIBETAN => Script::Tibetan,
            c::hb_script_t_HB_SCRIPT_BOPOMOFO => Script::Bopomofo,
            c::hb_script_t_HB_SCRIPT_BRAILLE => Script::Braille,
            c::hb_script_t_HB_SCRIPT_CANADIAN_SYLLABICS => Script::CanadianSyllabics,
            c::hb_script_t_HB_SCRIPT_CHEROKEE => Script::Cherokee,
            c::hb_script_t_HB_SCRIPT_ETHIOPIC => Script::Ethiopic,
            c::hb_script_t_HB_SCRIPT_KHMER => Script::Khmer,
            c::hb_script_t_HB_SCRIPT_MONGOLIAN => Script::Mongolian,
            c::hb_script_t_HB_SCRIPT_MYANMAR => Script::Myanmar,
            c::hb_script_t_HB_SCRIPT_OGHAM => Script::Ogham,
            c::hb_script_t_HB_SCRIPT_RUNIC => Script::Runic,
            c::hb_script_t_HB_SCRIPT_SINHALA => Script::Sinhala,
            c::hb_script_t_HB_SCRIPT_SYRIAC => Script::Syriac,
            c::hb_script_t_HB_SCRIPT_THAANA => Script::Thaana,
            c::hb_script_t_HB_SCRIPT_YI => Script::Yi,
            c::hb_script_t_HB_SCRIPT_DESERET => Script::Deseret,
            c::hb_script_t_HB_SCRIPT_GOTHIC => Script::Gothic,
            c::hb_script_t_HB_SCRIPT_OLD_ITALIC => Script::OldItalic,
            c::hb_script_t_HB_SCRIPT_BUHID => Script::Buhid,
            c::hb_script_t_HB_SCRIPT_HANUNOO => Script::Hanunoo,
            c::hb_script_t_HB_SCRIPT_TAGALOG => Script::Tagalog,
            c::hb_script_t_HB_SCRIPT_TAGBANWA => Script::Tagbanwa,
            c::hb_script_t_HB_SCRIPT_CYPRIOT => Script::Cypriot,
            c::hb_script_t_HB_SCRIPT_LIMBU => Script::Limbu,
            c::hb_script_t_HB_SCRIPT_LINEAR_B => Script::LinearB,
            c::hb_script_t_HB_SCRIPT_OSMANYA => Script::Osmanya,
            c::hb_script_t_HB_SCRIPT_SHAVIAN => Script::Shavian,
            c::hb_script_t_HB_SCRIPT_TAI_LE => Script::TaiLe,
            c::hb_script_t_HB_SCRIPT_UGARITIC => Script::Ugaritic,
            c::hb_script_t_HB_SCRIPT_BUGINESE => Script::Buginese,
            c::hb_script_t_HB_SCRIPT_COPTIC => Script::Coptic,
            c::hb_script_t_HB_SCRIPT_GLAGOLITIC => Script::Glagolitic,
            c::hb_script_t_HB_SCRIPT_KHAROSHTHI => Script::Kharoshthi,
            c::hb_script_t_HB_SCRIPT_NEW_TAI_LUE => Script::NewTaiLue,
            c::hb_script_t_HB_SCRIPT_OLD_PERSIAN => Script::OldPersian,
            c::hb_script_t_HB_SCRIPT_SYLOTI_NAGRI => Script::SylotiNagri,
            c::hb_script_t_HB_SCRIPT_TIFINAGH => Script::Tifinagh,
            c::hb_script_t_HB_SCRIPT_BALINESE => Script::Balinese,
            c::hb_script_t_HB_SCRIPT_CUNEIFORM => Script::Cuneiform,
            c::hb_script_t_HB_SCRIPT_NKO => Script::Nko,
            c::hb_script_t_HB_SCRIPT_PHAGS_PA => Script::PhagsPa,
            c::hb_script_t_HB_SCRIPT_PHOENICIAN => Script::Phoenician,
            c::hb_script_t_HB_SCRIPT_CARIAN => Script::Carian,
            c::hb_script_t_HB_SCRIPT_CHAM => Script::Cham,
            c::hb_script_t_HB_SCRIPT_KAYAH_LI => Script::KayahLi,
            c::hb_script_t_HB_SCRIPT_LEPCHA => Script::Lepcha,
            c::hb_script_t_HB_SCRIPT_LYCIAN => Script::Lycian,
            c::hb_script_t_HB_SCRIPT_LYDIAN => Script::Lydian,
            c::hb_script_t_HB_SCRIPT_OL_CHIKI => Script::OlChiki,
            c::hb_script_t_HB_SCRIPT_REJANG => Script::Rejang,
            c::hb_script_t_HB_SCRIPT_SAURASHTRA => Script::Saurashtra,
            c::hb_script_t_HB_SCRIPT_SUNDANESE => Script::Sundanese,
            c::hb_script_t_HB_SCRIPT_VAI => Script::Vai,
            c::hb_script_t_HB_SCRIPT_AVESTAN => Script::Avestan,
            c::hb_script_t_HB_SCRIPT_BAMUM => Script::Bamum,
            c::hb_script_t_HB_SCRIPT_EGYPTIAN_HIEROGLYPHS => Script::EgyptianHieroglyphs,
            c::hb_script_t_HB_SCRIPT_IMPERIAL_ARAMAIC => Script::ImperialAramaic,
            c::hb_script_t_HB_SCRIPT_INSCRIPTIONAL_PAHLAVI => Script::InscriptionalPahlavi,
            c::hb_script_t_HB_SCRIPT_INSCRIPTIONAL_PARTHIAN => Script::InscriptionalParthian,
            c::hb_script_t_HB_SCRIPT_JAVANESE => Script::Javanese,
            c::hb_script_t_HB_SCRIPT_KAITHI => Script::Kaithi,
            c::hb_script_t_HB_SCRIPT_LISU => Script::Lisu,
            c::hb_script_t_HB_SCRIPT_MEETEI_MAYEK => Script::MeeteiMayek,
            c::hb_script_t_HB_SCRIPT_OLD_SOUTH_ARABIAN => Script::OldSouthArabian,
            c::hb_script_t_HB_SCRIPT_OLD_TURKIC => Script::OldTurkic,
            c::hb_script_t_HB_SCRIPT_SAMARITAN => Script::Samaritan,
            c::hb_script_t_HB_SCRIPT_TAI_THAM => Script::TaiTham,
            c::hb_script_t_HB_SCRIPT_TAI_VIET => Script::TaiViet,
            c::hb_script_t_HB_SCRIPT_BATAK => Script::Batak,
            c::hb_script_t_HB_SCRIPT_BRAHMI => Script::Brahmi,
            c::hb_script_t_HB_SCRIPT_MANDAIC => Script::Mandaic,
            c::hb_script_t_HB_SCRIPT_CHAKMA => Script::Chakma,
            c::hb_script_t_HB_SCRIPT_MEROITIC_CURSIVE => Script::MeroiticCursive,
            c::hb_script_t_HB_SCRIPT_MEROITIC_HIEROGLYPHS => Script::MeroiticHieroglyphs,
            c::hb_script_t_HB_SCRIPT_MIAO => Script::Miao,
            c::hb_script_t_HB_SCRIPT_SHARADA => Script::Sharada,
            c::hb_script_t_HB_SCRIPT_SORA_SOMPENG => Script::SoraSompeng,
            c::hb_script_t_HB_SCRIPT_TAKRI => Script::Takri,
            c::hb_script_t_HB_SCRIPT_BASSA_VAH => Script::BassaVah,
            c::hb_script_t_HB_SCRIPT_CAUCASIAN_ALBANIAN => Script::CaucasianAlbanian,
            c::hb_script_t_HB_SCRIPT_DUPLOYAN => Script::Duployan,
            c::hb_script_t_HB_SCRIPT_ELBASAN => Script::Elbasan,
            c::hb_script_t_HB_SCRIPT_GRANTHA => Script::Grantha,
            c::hb_script_t_HB_SCRIPT_KHOJKI => Script::Khojki,
            c::hb_script_t_HB_SCRIPT_KHUDAWADI => Script::Khudawadi,
            c::hb_script_t_HB_SCRIPT_LINEAR_A => Script::LinearA,
            c::hb_script_t_HB_SCRIPT_MAHAJANI => Script::Mahajani,
            c::hb_script_t_HB_SCRIPT_MANICHAEAN => Script::Manichaean,
            c::hb_script_t_HB_SCRIPT_MENDE_KIKAKUI => Script::MendeKikakui,
            c::hb_script_t_HB_SCRIPT_MODI => Script::Modi,
            c::hb_script_t_HB_SCRIPT_MRO => Script::Mro,
            c::hb_script_t_HB_SCRIPT_NABATAEAN => Script::Nabataean,
            c::hb_script_t_HB_SCRIPT_OLD_NORTH_ARABIAN => Script::OldNorthArabian,
            c::hb_script_t_HB_SCRIPT_OLD_PERMIC => Script::OldPermic,
            c::hb_script_t_HB_SCRIPT_PAHAWH_HMONG => Script::PahawhHmong,
            c::hb_script_t_HB_SCRIPT_PALMYRENE => Script::Palmyrene,
            c::hb_script_t_HB_SCRIPT_PAU_CIN_HAU => Script::PauCinHau,
            c::hb_script_t_HB_SCRIPT_PSALTER_PAHLAVI => Script::PsalterPahlavi,
            c::hb_script_t_HB_SCRIPT_SIDDHAM => Script::Siddham,
            c::hb_script_t_HB_SCRIPT_TIRHUTA => Script::Tirhuta,
            c::hb_script_t_HB_SCRIPT_WARANG_CITI => Script::WarangCiti,
            c::hb_script_t_HB_SCRIPT_AHOM => Script::Ahom,
            c::hb_script_t_HB_SCRIPT_ANATOLIAN_HIEROGLYPHS => Script::AnatolianHieroglyphs,
            c::hb_script_t_HB_SCRIPT_HATRAN => Script::Hatran,
            c::hb_script_t_HB_SCRIPT_MULTANI => Script::Multani,
            c::hb_script_t_HB_SCRIPT_OLD_HUNGARIAN => Script::OldHungarian,
            c::hb_script_t_HB_SCRIPT_SIGNWRITING => Script::Signwriting,
            c::hb_script_t_HB_SCRIPT_ADLAM => Script::Adlam,
            c::hb_script_t_HB_SCRIPT_BHAIKSUKI => Script::Bhaiksuki,
            c::hb_script_t_HB_SCRIPT_MARCHEN => Script::Marchen,
            c::hb_script_t_HB_SCRIPT_OSAGE => Script::Osage,
            c::hb_script_t_HB_SCRIPT_TANGUT => Script::Tangut,
            c::hb_script_t_HB_SCRIPT_NEWA => Script::Newa,
            c::hb_script_t_HB_SCRIPT_MASARAM_GONDI => Script::MasaramGondi,
            c::hb_script_t_HB_SCRIPT_NUSHU => Script::Nushu,
            c::hb_script_t_HB_SCRIPT_SOYOMBO => Script::Soyombo,
            c::hb_script_t_HB_SCRIPT_ZANABAZAR_SQUARE => Script::ZanabazarSquare,
            c::hb_script_t_HB_SCRIPT_DOGRA => Script::Dogra,
            c::hb_script_t_HB_SCRIPT_GUNJALA_GONDI => Script::GunjalaGondi,
            c::hb_script_t_HB_SCRIPT_HANIFI_ROHINGYA => Script::HanifiRohingya,
            c::hb_script_t_HB_SCRIPT_MAKASAR => Script::Makasar,
            c::hb_script_t_HB_SCRIPT_MEDEFAIDRIN => Script::Medefaidrin,
            c::hb_script_t_HB_SCRIPT_OLD_SOGDIAN => Script::OldSogdian,
            c::hb_script_t_HB_SCRIPT_SOGDIAN => Script::Sogdian,
            c::hb_script_t_HB_SCRIPT_ELYMAIC => Script::Elymaic,
            c::hb_script_t_HB_SCRIPT_NANDINAGARI => Script::Nandinagari,
            c::hb_script_t_HB_SCRIPT_NYIAKENG_PUACHUE_HMONG => Script::NyiakengPuachueHmong,
            c::hb_script_t_HB_SCRIPT_WANCHO => Script::Wancho,
            c::hb_script_t_HB_SCRIPT_CHORASMIAN => Script::Chorasmian,
            c::hb_script_t_HB_SCRIPT_DIVES_AKURU => Script::DivesAkuru,
            c::hb_script_t_HB_SCRIPT_KHITAN_SMALL_SCRIPT => Script::KhitanSmallScript,
            c::hb_script_t_HB_SCRIPT_YEZIDI => Script::Yezidi,
            c::hb_script_t_HB_SCRIPT_CYPRO_MINOAN => Script::CyproMinoan,
            c::hb_script_t_HB_SCRIPT_OLD_UYGHUR => Script::OldUyghur,
            c::hb_script_t_HB_SCRIPT_TANGSA => Script::Tangsa,
            c::hb_script_t_HB_SCRIPT_TOTO => Script::Toto,
            c::hb_script_t_HB_SCRIPT_VITHKUQI => Script::Vithkuqi,
            c::hb_script_t_HB_SCRIPT_MATH => Script::Math,
            c::hb_script_t_HB_SCRIPT_INVALID => Script::Invalid,
            _ => Script::Invalid,
        }
    }
}

/// Data type for languages. Each `hb_language_t` corresponds to a BCP 47
/// language tag.
// PORT NOTE: `common.zig`'s `Language`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Language {
    pub handle: c::hb_language_t,
}

impl Language {
    /// Converts str representing a BCP 47 language tag to the corresponding
    /// hb_language_t.
    // PORT NOTE: `Language.fromString`. `str` is a BYTE slice, as in the reference.
    pub fn from_string(str: &[u8]) -> Language {
        Language {
            handle: unsafe {
                c::hb_language_from_string(
                    str.as_ptr().cast::<core::ffi::c_char>(),
                    str.len() as core::ffi::c_int,
                )
            },
        }
    }

    /// Converts an hb_language_t to a string.
    // PORT NOTE: `Language.toString`. Returns the borrowed NUL-terminated `CStr`
    // (the reference's `[:0]const u8`), never a lossy `String`.
    #[allow(clippy::wrong_self_convention)]
    pub fn to_string(self) -> &'static core::ffi::CStr {
        unsafe { core::ffi::CStr::from_ptr(c::hb_language_to_string(self.handle)) }
    }

    /// Fetch the default language from current locale.
    // PORT NOTE: `Language.getDefault`.
    pub fn get_default() -> Language {
        Language {
            handle: unsafe { c::hb_language_get_default() },
        }
    }
}

/// The `Feature` constructors/formatters of `common.zig`'s `Feature`.
// PORT NOTE: rustty re-exports the bindgen `hb_feature_t` AS [`Feature`] (identical
// `extern struct` layout), and Rust forbids inherent impls on a foreign type, so
// the reference's `Feature.fromString`/`Feature.toString` land on this extension
// trait instead. Import it to call them.
pub trait FeatureExt: Sized {
    /// `pub fn fromString(str: []const u8) ?Feature`
    fn from_string(str: &[u8]) -> Option<Self>;

    /// `pub fn toString(self: *Feature, buf: []u8) void`
    fn to_string(&mut self, buf: &mut [u8]);
}

impl FeatureExt for Feature {
    fn from_string(str: &[u8]) -> Option<Feature> {
        // var f: c.hb_feature_t = undefined;
        let mut f: c::hb_feature_t = unsafe { core::mem::zeroed() };
        let ok = unsafe {
            c::hb_feature_from_string(
                str.as_ptr().cast::<core::ffi::c_char>(),
                str.len() as core::ffi::c_int,
                &mut f,
            )
        };
        if ok > 0 {
            Some(f)
        } else {
            None
        }
    }

    fn to_string(&mut self, buf: &mut [u8]) {
        unsafe {
            c::hb_feature_to_string(
                self,
                buf.as_mut_ptr().cast::<core::ffi::c_char>(),
                buf.len() as core::ffi::c_uint,
            );
        }
    }
}

// The `test { ... }` blocks of `pkg/harfbuzz` (7 total): `blob.zig:115`,
// `buffer.zig:340` "create", `common.zig:244` "feature from string",
// `freetype.zig:49`, `version.zig:34`, `main.zig:29`'s `refAllDecls`, and the
// macOS-only `coretext.zig:18`.
#[cfg(test)]
mod tests {
    use super::*;

    /// Every `Script` variant, in the reference declaration order.
    const ALL_SCRIPTS: [Script; 164] = [
        Script::Common,
        Script::Inherited,
        Script::Unknown,
        Script::Arabic,
        Script::Armenian,
        Script::Bengali,
        Script::Cyrillic,
        Script::Devanagari,
        Script::Georgian,
        Script::Greek,
        Script::Gujarati,
        Script::Gurmukhi,
        Script::Hangul,
        Script::Han,
        Script::Hebrew,
        Script::Hiragana,
        Script::Kannada,
        Script::Katakana,
        Script::Lao,
        Script::Latin,
        Script::Malayalam,
        Script::Oriya,
        Script::Tamil,
        Script::Telugu,
        Script::Thai,
        Script::Tibetan,
        Script::Bopomofo,
        Script::Braille,
        Script::CanadianSyllabics,
        Script::Cherokee,
        Script::Ethiopic,
        Script::Khmer,
        Script::Mongolian,
        Script::Myanmar,
        Script::Ogham,
        Script::Runic,
        Script::Sinhala,
        Script::Syriac,
        Script::Thaana,
        Script::Yi,
        Script::Deseret,
        Script::Gothic,
        Script::OldItalic,
        Script::Buhid,
        Script::Hanunoo,
        Script::Tagalog,
        Script::Tagbanwa,
        Script::Cypriot,
        Script::Limbu,
        Script::LinearB,
        Script::Osmanya,
        Script::Shavian,
        Script::TaiLe,
        Script::Ugaritic,
        Script::Buginese,
        Script::Coptic,
        Script::Glagolitic,
        Script::Kharoshthi,
        Script::NewTaiLue,
        Script::OldPersian,
        Script::SylotiNagri,
        Script::Tifinagh,
        Script::Balinese,
        Script::Cuneiform,
        Script::Nko,
        Script::PhagsPa,
        Script::Phoenician,
        Script::Carian,
        Script::Cham,
        Script::KayahLi,
        Script::Lepcha,
        Script::Lycian,
        Script::Lydian,
        Script::OlChiki,
        Script::Rejang,
        Script::Saurashtra,
        Script::Sundanese,
        Script::Vai,
        Script::Avestan,
        Script::Bamum,
        Script::EgyptianHieroglyphs,
        Script::ImperialAramaic,
        Script::InscriptionalPahlavi,
        Script::InscriptionalParthian,
        Script::Javanese,
        Script::Kaithi,
        Script::Lisu,
        Script::MeeteiMayek,
        Script::OldSouthArabian,
        Script::OldTurkic,
        Script::Samaritan,
        Script::TaiTham,
        Script::TaiViet,
        Script::Batak,
        Script::Brahmi,
        Script::Mandaic,
        Script::Chakma,
        Script::MeroiticCursive,
        Script::MeroiticHieroglyphs,
        Script::Miao,
        Script::Sharada,
        Script::SoraSompeng,
        Script::Takri,
        Script::BassaVah,
        Script::CaucasianAlbanian,
        Script::Duployan,
        Script::Elbasan,
        Script::Grantha,
        Script::Khojki,
        Script::Khudawadi,
        Script::LinearA,
        Script::Mahajani,
        Script::Manichaean,
        Script::MendeKikakui,
        Script::Modi,
        Script::Mro,
        Script::Nabataean,
        Script::OldNorthArabian,
        Script::OldPermic,
        Script::PahawhHmong,
        Script::Palmyrene,
        Script::PauCinHau,
        Script::PsalterPahlavi,
        Script::Siddham,
        Script::Tirhuta,
        Script::WarangCiti,
        Script::Ahom,
        Script::AnatolianHieroglyphs,
        Script::Hatran,
        Script::Multani,
        Script::OldHungarian,
        Script::Signwriting,
        Script::Adlam,
        Script::Bhaiksuki,
        Script::Marchen,
        Script::Osage,
        Script::Tangut,
        Script::Newa,
        Script::MasaramGondi,
        Script::Nushu,
        Script::Soyombo,
        Script::ZanabazarSquare,
        Script::Dogra,
        Script::GunjalaGondi,
        Script::HanifiRohingya,
        Script::Makasar,
        Script::Medefaidrin,
        Script::OldSogdian,
        Script::Sogdian,
        Script::Elymaic,
        Script::Nandinagari,
        Script::NyiakengPuachueHmong,
        Script::Wancho,
        Script::Chorasmian,
        Script::DivesAkuru,
        Script::KhitanSmallScript,
        Script::Yezidi,
        Script::CyproMinoan,
        Script::OldUyghur,
        Script::Tangsa,
        Script::Toto,
        Script::Vithkuqi,
        Script::Math,
        Script::Invalid,
    ];

    // test { ... } — `version.zig:34`.
    #[test]
    fn version_basics() {
        // Should be able to get the version
        let vsn = version();
        assert!(vsn.major > 0);

        // Should be at least version 1
        assert!(version_at_least(Version {
            major: 1,
            minor: 0,
            micro: 0,
        }));

        // PORT NOTE (additive): `versionString` has no reference assertion; pin it
        // to the same triple so the two accessors cannot disagree.
        assert_eq!(
            format!("{}.{}.{}", vsn.major, vsn.minor, vsn.micro).as_bytes(),
            version_string().to_bytes()
        );
    }

    // test { ... } — `blob.zig:115`.
    #[test]
    fn blob_user_data() {
        // const data = "hello";
        let data = b"hello";
        // var blob = try Blob.create(data, .readonly);
        let blob = Blob::create(data, MemoryMode::Readonly).unwrap();

        // var userdata: u8 = 127; var key: u8 = 0;
        let mut userdata: u8 = 127;
        // PORT NOTE: the reference passes `&key` (a `*u8`) through `@ptrCast` to the
        // `?*anyopaque` key; HarfBuzz only ever compares the key by ADDRESS, so the
        // Rust binding's typed `*mut hb_user_data_key_t` is fed the same way.
        let mut key = c::hb_user_data_key_t { unused: 0 };
        let key: *mut c::hb_user_data_key_t = &mut key;

        // try testing.expect(blob.setUserData(u8, &key, &userdata, null, false));
        assert!(unsafe { blob.set_user_data::<u8>(key, &mut userdata, None, false) });
        // try testing.expect(blob.getUserData(u8, &key).?.* == 127);
        assert_eq!(127, unsafe { *blob.get_user_data::<u8>(key).unwrap() });

        // PORT NOTE (additive): an unknown key must not resolve to the stored entry.
        let mut other = c::hb_user_data_key_t { unused: 0 };
        assert!(unsafe { blob.get_user_data::<u8>(&mut other) }.is_none());

        blob.destroy();
    }

    // test "feature from string" — `common.zig:244`.
    #[test]
    fn feature_from_string() {
        // try testing.expect(Feature.fromString("dlig") != null);
        assert!(Feature::from_string(b"dlig").is_some());

        // PORT NOTE (additive): round-trip through `toString`, the other half of the
        // reference declaration pair, which its own test never exercises.
        let mut f = Feature::from_string(b"dlig").unwrap();
        let mut buf = [0u8; 128];
        f.to_string(&mut buf);
        let n = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
        assert_eq!(b"dlig", &buf[..n]);

        // An unparseable feature string yields `null` (HarfBuzz's parser is very
        // lenient — it accepts almost any 4-byte tag — but rejects an empty one).
        assert!(Feature::from_string(b"").is_none());
    }

    // test "create" — `buffer.zig:340`.
    #[test]
    fn buffer_create() {
        // var buffer = try Buffer.create(); defer buffer.destroy();
        let buffer = Buffer::create().unwrap();
        buffer.reset();

        // Content type
        buffer.set_content_type(ContentType::Unicode);
        assert_eq!(ContentType::Unicode, buffer.get_content_type());

        // Try add functions
        // buffer.add('🥹', 27);
        buffer.add('🥹' as u32, 27);
        let utf32 = ['A' as u32, 'B' as u32, 'C' as u32];
        let utf16 = ['A' as u16, 'B' as u16, 'C' as u16];
        let utf8 = [b'A', b'B', b'C'];
        buffer.add_codepoints(&utf32);
        buffer.add_utf32(&utf32);
        buffer.add_utf16(&utf16);
        buffer.add_utf8(&utf8);
        buffer.add_latin1(&utf8);

        // Guess properties first
        buffer.guess_segment_properties();

        // Try to set properties
        buffer.set_direction(Direction::Ltr);
        assert_eq!(Direction::Ltr, buffer.get_direction());

        buffer.set_script(Script::Arabic);
        assert_eq!(Script::Arabic, buffer.get_script());

        buffer.set_language(Language::from_string(b"en"));

        // PORT NOTE (additive): the reference stops at `setLanguage`; assert the
        // matching getter round-trips, and that every added item landed.
        assert_eq!(b"en", buffer.get_language().to_string().to_bytes());
        assert_eq!(16, buffer.get_length());

        buffer.destroy();
    }

    // test { ... } — `freetype.zig:49`.
    //
    // PORT NOTE: the reference reaches for the raw `freetype.c` API because
    // `pkg/harfbuzz` has no Zig-side FreeType wrapper dependency; rustty's `ft`
    // module is a sibling in the same crate, so the identical sequence
    // (init library → memory face → createFace → createFont → setFontFuncs) is
    // spelled with the ported wrappers.
    #[test]
    fn freetype_bridge() {
        let test_font = crate::ft::testing::FONT_REGULAR;

        let ft_lib = crate::ft::Library::init().unwrap();
        let ft_face = ft_lib.init_memory_face(test_font, 0).unwrap();

        // var face = try createFace(ft_face); defer face.destroy();
        let face = create_face(ft_face.handle).unwrap();
        // var font = try createFont(ft_face); defer font.destroy();
        let font = create_font(ft_face.handle).unwrap();
        // setFontFuncs(font);
        set_font_funcs(font);

        // PORT NOTE (additive): `Font::create` (`font.zig:12`) has no reference test
        // of its own; build one from the face the bridge just returned.
        let font2 = Font::create(face).unwrap();
        font2.destroy();

        font.destroy();
        face.destroy();
        ft_face.deinit();
        ft_lib.deinit();
    }

    // test { ... } — `coretext.zig:18` (macOS only, exactly as the reference file).
    #[cfg(target_os = "macos")]
    #[test]
    fn coretext_bridge() {
        use objc2_core_foundation::{CFRetained, CFString, CFStringBuiltInEncodings};
        use objc2_core_text::{CTFont, CTFontDescriptor};

        // const name = try macos.foundation.String.createWithBytes("Monaco", .utf8, false);
        let name = unsafe {
            CFString::with_bytes(
                None,
                b"Monaco".as_ptr(),
                6,
                CFStringBuiltInEncodings::EncodingUTF8.0,
                false,
            )
        }
        .unwrap();
        // const desc = try macos.text.FontDescriptor.createWithNameAndSize(name, 12);
        let desc = unsafe { CTFontDescriptor::with_name_and_size(&name, 12.0) };
        // const font = try macos.text.Font.createWithFontDescriptor(desc, 12);
        let font: CFRetained<CTFont> =
            unsafe { CTFont::with_font_descriptor(&desc, 12.0, core::ptr::null()) };

        // var hb_font = try createFont(font); defer hb_font.destroy();
        let hb_font =
            create_font_coretext(CFRetained::as_ptr(&font).as_ptr() as *const core::ffi::c_void)
                .unwrap();
        hb_font.destroy();
    }

    // test { @import("std").testing.refAllDecls(@This()); } — `main.zig:29`.
    #[test]
    fn ref_all_decls() {
        let _ = core::mem::size_of::<MemoryMode>();
        let _ = core::mem::size_of::<Blob>();
        let _ = core::mem::size_of::<Buffer>();
        let _ = core::mem::size_of::<GlyphInfo>();
        let _ = core::mem::size_of::<GlyphPosition>();
        let _ = core::mem::size_of::<Direction>();
        let _ = core::mem::size_of::<Script>();
        let _ = core::mem::size_of::<Language>();
        let _ = core::mem::size_of::<Feature>();
        let _ = core::mem::size_of::<Face>();
        let _ = core::mem::size_of::<Font>();
        let _ = core::mem::size_of::<Version>();
        let _ = core::mem::size_of::<ContentType>();
        let _ = core::mem::size_of::<ClusterLevel>();
        let _ = core::mem::size_of::<HarfbuzzFailed>();
        let _ = HB_FEATURE_GLOBAL_START;
        let _ = HB_FEATURE_GLOBAL_END;

        let _ = Blob::create as fn(&[u8], MemoryMode) -> Result<Blob, HarfbuzzFailed>;
        let _ = Blob::destroy as fn(Blob);
        let _ = Blob::set_user_data::<u8>
            as unsafe fn(
                Blob,
                *mut c::hb_user_data_key_t,
                *mut u8,
                Option<unsafe extern "C" fn(*mut u8)>,
                bool,
            ) -> bool;
        let _ = Blob::get_user_data::<u8>
            as unsafe fn(Blob, *mut c::hb_user_data_key_t) -> Option<*mut u8>;
        let _ = Face::destroy as fn(Face);
        let _ = Font::create as fn(Face) -> Result<Font, HarfbuzzFailed>;
        let _ = Font::destroy as fn(Font);
        let _ = Font::set_scale as fn(Font, u32, u32);
        let _ = Buffer::create as fn() -> Result<Buffer, HarfbuzzFailed>;
        let _ = Buffer::destroy as fn(Buffer);
        let _ = Buffer::reset as fn(Buffer);
        let _ = Buffer::get_length as fn(Buffer) -> u32;
        let _ = Buffer::set_content_type as fn(Buffer, ContentType);
        let _ = Buffer::get_content_type as fn(Buffer) -> ContentType;
        let _ = Buffer::add as fn(Buffer, u32, u32);
        let _ = Buffer::add_codepoints as fn(Buffer, &[u32]);
        let _ = Buffer::add_utf32 as fn(Buffer, &[u32]);
        let _ = Buffer::add_utf16 as fn(Buffer, &[u16]);
        let _ = Buffer::add_utf8 as fn(Buffer, &[u8]);
        let _ = Buffer::add_latin1 as fn(Buffer, &[u8]);
        let _ = Buffer::set_direction as fn(Buffer, Direction);
        let _ = Buffer::get_direction as fn(Buffer) -> Direction;
        let _ = Buffer::set_script as fn(Buffer, Script);
        let _ = Buffer::get_script as fn(Buffer) -> Script;
        let _ = Buffer::set_language as fn(Buffer, Language);
        let _ = Buffer::get_language as fn(Buffer) -> Language;
        let _ = Buffer::guess_segment_properties as fn(Buffer);
        let _ = Buffer::set_cluster_level as fn(Buffer, ClusterLevel);
        let _ = Language::from_string as fn(&[u8]) -> Language;
        let _ = Language::to_string as fn(Language) -> &'static core::ffi::CStr;
        let _ = Language::get_default as fn() -> Language;
        let _ = <Feature as FeatureExt>::from_string as fn(&[u8]) -> Option<Feature>;
        let _ = <Feature as FeatureExt>::to_string as fn(&mut Feature, &mut [u8]);
        let _ = shape as fn(Font, Buffer, Option<&[Feature]>);
        let _ = version as fn() -> Version;
        let _ = version_at_least as fn(Version) -> bool;
        let _ = version_string as fn() -> &'static core::ffi::CStr;
        let _ = create_face as fn(freetype_sys::FT_Face) -> Result<Face, HarfbuzzFailed>;
        let _ = create_font as fn(freetype_sys::FT_Face) -> Result<Font, HarfbuzzFailed>;
        let _ = set_font_funcs as fn(Font);
        #[cfg(target_os = "macos")]
        let _ =
            create_font_coretext as fn(*const core::ffi::c_void) -> Result<Font, HarfbuzzFailed>;

        // The 164-variant `Script` table maps to 164 distinct raw tags.
        let all = ALL_SCRIPTS;
        assert_eq!(164, all.len());
        for (i, a) in all.iter().enumerate() {
            assert_eq!(*a, Script::from_c(a.to_c()));
            for b in &all[i + 1..] {
                assert_ne!(a.to_c(), b.to_c());
            }
        }
    }
}
