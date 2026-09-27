//! Port of the reference `freetype` package
//! (`pkg/freetype/{main,Library,face,tag,computations,errors,test,c}.zig`) — a
//! hand-written Zig wrapper over the FreeType C API, here implemented as `unsafe`
//! wrappers over `freetype-sys`.
//!
//! The whole pinned package surface is carved: [`Library`] (+ [`Version`]),
//! [`Face`], [`LoadFlags`] (+ [`Target`]), [`Tag`], [`Encoding`], [`RenderMode`],
//! [`SfntTag`], [`mul_fix`], the named [`FtError`] set with [`int_to_error`] /
//! [`error_to_int`], and the [`testing`] fixture module. All 7 of the package's
//! `test { ... }` blocks are ported below (`errors.zig` "error conversion",
//! `Library.zig` "basics", `face.zig`'s three `LoadFlags` tables + "loading
//! memory font", and `main.zig`'s `refAllDecls` reification).
//!
//! PORT NOTE (`c.zig`): the reference re-exports the raw C namespace as
//! `freetype.c`; rustty's equivalent is the `freetype_sys` crate itself, which
//! callers (`face/freetype.rs`) import directly.
//!
//! PORT NOTE (`getSfntTable`): Zig's `comptime tag: SfntTag` + `tag.DataType()`
//! returns a differently-typed pointer per tag. Rust has no comptime return-type
//! dispatch, so [`Face`] exposes one typed accessor per tag that the reference's
//! `DataType()` actually models (`head`/`os2`/`post`/`hhea` — the others are
//! `unreachable` there).

use freetype_sys as c;

/// The FreeType error set.
//
// PORT NOTE: `pkg/freetype/errors.zig`'s `Error` set, ported variant-for-variant
// (89 mapped codes + the `UnknownFreetypeError` catch-all). Named `FtError` (not
// bare `Error`) because this module is a submodule of the shaper crate rather
// than its own `freetype` package namespace; `Error` below is the alias that
// mirrors `main.zig:10`'s `pub const Error = errors.Error;`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FtError {
    CannotOpenResource,
    UnknownFileFormat,
    InvalidFileFormat,
    InvalidVersion,
    LowerModuleVersion,
    InvalidArgument,
    UnimplementedFeature,
    InvalidTable,
    InvalidOffset,
    ArrayTooLarge,
    MissingModule,
    MissingProperty,
    InvalidGlyphIndex,
    InvalidCharacterCode,
    InvalidGlyphFormat,
    CannotRenderGlyph,
    InvalidOutline,
    InvalidComposite,
    TooManyHints,
    InvalidPixelSize,
    InvalidHandle,
    InvalidLibraryHandle,
    InvalidDriverHandle,
    InvalidFaceHandle,
    InvalidSizeHandle,
    InvalidSlotHandle,
    InvalidCharMapHandle,
    InvalidCacheHandle,
    InvalidStreamHandle,
    TooManyDrivers,
    TooManyExtensions,
    OutOfMemory,
    UnlistedObject,
    CannotOpenStream,
    InvalidStreamSeek,
    InvalidStreamSkip,
    InvalidStreamRead,
    InvalidStreamOperation,
    InvalidFrameOperation,
    NestedFrameAccess,
    InvalidFrameRead,
    RasterUninitialized,
    RasterCorrupted,
    RasterOverflow,
    RasterNegativeHeight,
    TooManyCaches,
    InvalidOpcode,
    TooFewArguments,
    StackOverflow,
    CodeOverflow,
    BadArgument,
    DivideByZero,
    InvalidReference,
    DebugOpCode,
    ENDFInExecStream,
    NestedDEFS,
    InvalidCodeRange,
    ExecutionTooLong,
    TooManyFunctionDefs,
    TooManyInstructionDefs,
    TableMissing,
    HorizHeaderMissing,
    LocationsMissing,
    NameTableMissing,
    CMapTableMissing,
    HmtxTableMissing,
    PostTableMissing,
    InvalidHorizMetrics,
    InvalidCharMapFormat,
    InvalidPPem,
    InvalidVertMetrics,
    CouldNotFindContext,
    InvalidPostTableFormat,
    InvalidPostTable,
    Syntax,
    StackUnderflow,
    Ignore,
    NoUnicodeGlyphName,
    MissingStartfontField,
    MissingFontField,
    MissingSizeField,
    MissingFontboundingboxField,
    MissingCharsField,
    MissingStartcharField,
    MissingEncodingField,
    MissingBbxField,
    BbxTooBig,
    CorruptedFontHeader,
    CorruptedFontGlyphs,
    UnknownFreetypeError,
}

/// `pub const Error = errors.Error;` (`main.zig:10`).
pub type Error = FtError;

/// Map a raw `FT_Error` to `Result`: `FT_Err_Ok` (0) → `Ok`, else the named error.
// PORT NOTE: `errors.intToError`.
pub fn int_to_error(err: c::FT_Error) -> Result<(), FtError> {
    // The bindgen `FT_Err_*` constants have the type of FreeType's anonymous
    // error enum, which bindgen gives the C compiler's underlying type for it:
    // `c_uint` on macOS/Linux, `c_int` on MSVC (measured on Windows, task W-1).
    // `FT_Error` is `c_int`. Convert to the constants' own type so they can be
    // used as patterns (all FreeType error codes are small positive values; a
    // value that does not convert is not one of them).
    fn like<T: TryFrom<c::FT_Error>>(_: T, v: c::FT_Error) -> Option<T> {
        T::try_from(v).ok()
    }
    let Some(code) = like(c::FT_Err_Ok, err) else {
        return Err(FtError::UnknownFreetypeError);
    };
    Err(match code {
        c::FT_Err_Ok => return Ok(()),
        c::FT_Err_Cannot_Open_Resource => FtError::CannotOpenResource,
        c::FT_Err_Unknown_File_Format => FtError::UnknownFileFormat,
        c::FT_Err_Invalid_File_Format => FtError::InvalidFileFormat,
        c::FT_Err_Invalid_Version => FtError::InvalidVersion,
        c::FT_Err_Lower_Module_Version => FtError::LowerModuleVersion,
        c::FT_Err_Invalid_Argument => FtError::InvalidArgument,
        c::FT_Err_Unimplemented_Feature => FtError::UnimplementedFeature,
        c::FT_Err_Invalid_Table => FtError::InvalidTable,
        c::FT_Err_Invalid_Offset => FtError::InvalidOffset,
        c::FT_Err_Array_Too_Large => FtError::ArrayTooLarge,
        c::FT_Err_Missing_Module => FtError::MissingModule,
        c::FT_Err_Missing_Property => FtError::MissingProperty,
        c::FT_Err_Invalid_Glyph_Index => FtError::InvalidGlyphIndex,
        c::FT_Err_Invalid_Character_Code => FtError::InvalidCharacterCode,
        c::FT_Err_Invalid_Glyph_Format => FtError::InvalidGlyphFormat,
        c::FT_Err_Cannot_Render_Glyph => FtError::CannotRenderGlyph,
        c::FT_Err_Invalid_Outline => FtError::InvalidOutline,
        c::FT_Err_Invalid_Composite => FtError::InvalidComposite,
        c::FT_Err_Too_Many_Hints => FtError::TooManyHints,
        c::FT_Err_Invalid_Pixel_Size => FtError::InvalidPixelSize,
        c::FT_Err_Invalid_Handle => FtError::InvalidHandle,
        c::FT_Err_Invalid_Library_Handle => FtError::InvalidLibraryHandle,
        c::FT_Err_Invalid_Driver_Handle => FtError::InvalidDriverHandle,
        c::FT_Err_Invalid_Face_Handle => FtError::InvalidFaceHandle,
        c::FT_Err_Invalid_Size_Handle => FtError::InvalidSizeHandle,
        c::FT_Err_Invalid_Slot_Handle => FtError::InvalidSlotHandle,
        c::FT_Err_Invalid_CharMap_Handle => FtError::InvalidCharMapHandle,
        c::FT_Err_Invalid_Cache_Handle => FtError::InvalidCacheHandle,
        c::FT_Err_Invalid_Stream_Handle => FtError::InvalidStreamHandle,
        c::FT_Err_Too_Many_Drivers => FtError::TooManyDrivers,
        c::FT_Err_Too_Many_Extensions => FtError::TooManyExtensions,
        c::FT_Err_Out_Of_Memory => FtError::OutOfMemory,
        c::FT_Err_Unlisted_Object => FtError::UnlistedObject,
        c::FT_Err_Cannot_Open_Stream => FtError::CannotOpenStream,
        c::FT_Err_Invalid_Stream_Seek => FtError::InvalidStreamSeek,
        c::FT_Err_Invalid_Stream_Skip => FtError::InvalidStreamSkip,
        c::FT_Err_Invalid_Stream_Read => FtError::InvalidStreamRead,
        c::FT_Err_Invalid_Stream_Operation => FtError::InvalidStreamOperation,
        c::FT_Err_Invalid_Frame_Operation => FtError::InvalidFrameOperation,
        c::FT_Err_Nested_Frame_Access => FtError::NestedFrameAccess,
        c::FT_Err_Invalid_Frame_Read => FtError::InvalidFrameRead,
        c::FT_Err_Raster_Uninitialized => FtError::RasterUninitialized,
        c::FT_Err_Raster_Corrupted => FtError::RasterCorrupted,
        c::FT_Err_Raster_Overflow => FtError::RasterOverflow,
        c::FT_Err_Raster_Negative_Height => FtError::RasterNegativeHeight,
        c::FT_Err_Too_Many_Caches => FtError::TooManyCaches,
        c::FT_Err_Invalid_Opcode => FtError::InvalidOpcode,
        c::FT_Err_Too_Few_Arguments => FtError::TooFewArguments,
        c::FT_Err_Stack_Overflow => FtError::StackOverflow,
        c::FT_Err_Code_Overflow => FtError::CodeOverflow,
        c::FT_Err_Bad_Argument => FtError::BadArgument,
        c::FT_Err_Divide_By_Zero => FtError::DivideByZero,
        c::FT_Err_Invalid_Reference => FtError::InvalidReference,
        c::FT_Err_Debug_OpCode => FtError::DebugOpCode,
        c::FT_Err_ENDF_In_Exec_Stream => FtError::ENDFInExecStream,
        c::FT_Err_Nested_DEFS => FtError::NestedDEFS,
        c::FT_Err_Invalid_CodeRange => FtError::InvalidCodeRange,
        c::FT_Err_Execution_Too_Long => FtError::ExecutionTooLong,
        c::FT_Err_Too_Many_Function_Defs => FtError::TooManyFunctionDefs,
        c::FT_Err_Too_Many_Instruction_Defs => FtError::TooManyInstructionDefs,
        c::FT_Err_Table_Missing => FtError::TableMissing,
        c::FT_Err_Horiz_Header_Missing => FtError::HorizHeaderMissing,
        c::FT_Err_Locations_Missing => FtError::LocationsMissing,
        c::FT_Err_Name_Table_Missing => FtError::NameTableMissing,
        c::FT_Err_CMap_Table_Missing => FtError::CMapTableMissing,
        c::FT_Err_Hmtx_Table_Missing => FtError::HmtxTableMissing,
        c::FT_Err_Post_Table_Missing => FtError::PostTableMissing,
        c::FT_Err_Invalid_Horiz_Metrics => FtError::InvalidHorizMetrics,
        c::FT_Err_Invalid_CharMap_Format => FtError::InvalidCharMapFormat,
        c::FT_Err_Invalid_PPem => FtError::InvalidPPem,
        c::FT_Err_Invalid_Vert_Metrics => FtError::InvalidVertMetrics,
        c::FT_Err_Could_Not_Find_Context => FtError::CouldNotFindContext,
        c::FT_Err_Invalid_Post_Table_Format => FtError::InvalidPostTableFormat,
        c::FT_Err_Invalid_Post_Table => FtError::InvalidPostTable,
        c::FT_Err_Syntax_Error => FtError::Syntax,
        c::FT_Err_Stack_Underflow => FtError::StackUnderflow,
        c::FT_Err_Ignore => FtError::Ignore,
        c::FT_Err_No_Unicode_Glyph_Name => FtError::NoUnicodeGlyphName,
        c::FT_Err_Missing_Startfont_Field => FtError::MissingStartfontField,
        c::FT_Err_Missing_Font_Field => FtError::MissingFontField,
        c::FT_Err_Missing_Size_Field => FtError::MissingSizeField,
        c::FT_Err_Missing_Fontboundingbox_Field => FtError::MissingFontboundingboxField,
        c::FT_Err_Missing_Chars_Field => FtError::MissingCharsField,
        c::FT_Err_Missing_Startchar_Field => FtError::MissingStartcharField,
        c::FT_Err_Missing_Encoding_Field => FtError::MissingEncodingField,
        c::FT_Err_Missing_Bbx_Field => FtError::MissingBbxField,
        c::FT_Err_Bbx_Too_Big => FtError::BbxTooBig,
        c::FT_Err_Corrupted_Font_Header => FtError::CorruptedFontHeader,
        c::FT_Err_Corrupted_Font_Glyphs => FtError::CorruptedFontGlyphs,
        _ => FtError::UnknownFreetypeError,
    })
}

/// Map a named error back to its raw `FT_Error` code.
// PORT NOTE: `errors.errorToInt`. The reference switch omits `UnknownFreetypeError`
// (it is never analyzed by the Zig compiler — `errorToInt` is not reachable from
// `main.zig`'s `refAllDecls`, so the non-exhaustive switch never trips). Rust
// requires exhaustiveness, so that one variant maps to `FT_Err_Max`, the sentinel
// FreeType places one past the last real error code — it is not any real error.
pub fn error_to_int(err: FtError) -> c::FT_Error {
    (match err {
        FtError::CannotOpenResource => c::FT_Err_Cannot_Open_Resource,
        FtError::UnknownFileFormat => c::FT_Err_Unknown_File_Format,
        FtError::InvalidFileFormat => c::FT_Err_Invalid_File_Format,
        FtError::InvalidVersion => c::FT_Err_Invalid_Version,
        FtError::LowerModuleVersion => c::FT_Err_Lower_Module_Version,
        FtError::InvalidArgument => c::FT_Err_Invalid_Argument,
        FtError::UnimplementedFeature => c::FT_Err_Unimplemented_Feature,
        FtError::InvalidTable => c::FT_Err_Invalid_Table,
        FtError::InvalidOffset => c::FT_Err_Invalid_Offset,
        FtError::ArrayTooLarge => c::FT_Err_Array_Too_Large,
        FtError::MissingModule => c::FT_Err_Missing_Module,
        FtError::MissingProperty => c::FT_Err_Missing_Property,
        FtError::InvalidGlyphIndex => c::FT_Err_Invalid_Glyph_Index,
        FtError::InvalidCharacterCode => c::FT_Err_Invalid_Character_Code,
        FtError::InvalidGlyphFormat => c::FT_Err_Invalid_Glyph_Format,
        FtError::CannotRenderGlyph => c::FT_Err_Cannot_Render_Glyph,
        FtError::InvalidOutline => c::FT_Err_Invalid_Outline,
        FtError::InvalidComposite => c::FT_Err_Invalid_Composite,
        FtError::TooManyHints => c::FT_Err_Too_Many_Hints,
        FtError::InvalidPixelSize => c::FT_Err_Invalid_Pixel_Size,
        FtError::InvalidHandle => c::FT_Err_Invalid_Handle,
        FtError::InvalidLibraryHandle => c::FT_Err_Invalid_Library_Handle,
        FtError::InvalidDriverHandle => c::FT_Err_Invalid_Driver_Handle,
        FtError::InvalidFaceHandle => c::FT_Err_Invalid_Face_Handle,
        FtError::InvalidSizeHandle => c::FT_Err_Invalid_Size_Handle,
        FtError::InvalidSlotHandle => c::FT_Err_Invalid_Slot_Handle,
        FtError::InvalidCharMapHandle => c::FT_Err_Invalid_CharMap_Handle,
        FtError::InvalidCacheHandle => c::FT_Err_Invalid_Cache_Handle,
        FtError::InvalidStreamHandle => c::FT_Err_Invalid_Stream_Handle,
        FtError::TooManyDrivers => c::FT_Err_Too_Many_Drivers,
        FtError::TooManyExtensions => c::FT_Err_Too_Many_Extensions,
        FtError::OutOfMemory => c::FT_Err_Out_Of_Memory,
        FtError::UnlistedObject => c::FT_Err_Unlisted_Object,
        FtError::CannotOpenStream => c::FT_Err_Cannot_Open_Stream,
        FtError::InvalidStreamSeek => c::FT_Err_Invalid_Stream_Seek,
        FtError::InvalidStreamSkip => c::FT_Err_Invalid_Stream_Skip,
        FtError::InvalidStreamRead => c::FT_Err_Invalid_Stream_Read,
        FtError::InvalidStreamOperation => c::FT_Err_Invalid_Stream_Operation,
        FtError::InvalidFrameOperation => c::FT_Err_Invalid_Frame_Operation,
        FtError::NestedFrameAccess => c::FT_Err_Nested_Frame_Access,
        FtError::InvalidFrameRead => c::FT_Err_Invalid_Frame_Read,
        FtError::RasterUninitialized => c::FT_Err_Raster_Uninitialized,
        FtError::RasterCorrupted => c::FT_Err_Raster_Corrupted,
        FtError::RasterOverflow => c::FT_Err_Raster_Overflow,
        FtError::RasterNegativeHeight => c::FT_Err_Raster_Negative_Height,
        FtError::TooManyCaches => c::FT_Err_Too_Many_Caches,
        FtError::InvalidOpcode => c::FT_Err_Invalid_Opcode,
        FtError::TooFewArguments => c::FT_Err_Too_Few_Arguments,
        FtError::StackOverflow => c::FT_Err_Stack_Overflow,
        FtError::CodeOverflow => c::FT_Err_Code_Overflow,
        FtError::BadArgument => c::FT_Err_Bad_Argument,
        FtError::DivideByZero => c::FT_Err_Divide_By_Zero,
        FtError::InvalidReference => c::FT_Err_Invalid_Reference,
        FtError::DebugOpCode => c::FT_Err_Debug_OpCode,
        FtError::ENDFInExecStream => c::FT_Err_ENDF_In_Exec_Stream,
        FtError::NestedDEFS => c::FT_Err_Nested_DEFS,
        FtError::InvalidCodeRange => c::FT_Err_Invalid_CodeRange,
        FtError::ExecutionTooLong => c::FT_Err_Execution_Too_Long,
        FtError::TooManyFunctionDefs => c::FT_Err_Too_Many_Function_Defs,
        FtError::TooManyInstructionDefs => c::FT_Err_Too_Many_Instruction_Defs,
        FtError::TableMissing => c::FT_Err_Table_Missing,
        FtError::HorizHeaderMissing => c::FT_Err_Horiz_Header_Missing,
        FtError::LocationsMissing => c::FT_Err_Locations_Missing,
        FtError::NameTableMissing => c::FT_Err_Name_Table_Missing,
        FtError::CMapTableMissing => c::FT_Err_CMap_Table_Missing,
        FtError::HmtxTableMissing => c::FT_Err_Hmtx_Table_Missing,
        FtError::PostTableMissing => c::FT_Err_Post_Table_Missing,
        FtError::InvalidHorizMetrics => c::FT_Err_Invalid_Horiz_Metrics,
        FtError::InvalidCharMapFormat => c::FT_Err_Invalid_CharMap_Format,
        FtError::InvalidPPem => c::FT_Err_Invalid_PPem,
        FtError::InvalidVertMetrics => c::FT_Err_Invalid_Vert_Metrics,
        FtError::CouldNotFindContext => c::FT_Err_Could_Not_Find_Context,
        FtError::InvalidPostTableFormat => c::FT_Err_Invalid_Post_Table_Format,
        FtError::InvalidPostTable => c::FT_Err_Invalid_Post_Table,
        FtError::Syntax => c::FT_Err_Syntax_Error,
        FtError::StackUnderflow => c::FT_Err_Stack_Underflow,
        FtError::Ignore => c::FT_Err_Ignore,
        FtError::NoUnicodeGlyphName => c::FT_Err_No_Unicode_Glyph_Name,
        FtError::MissingStartfontField => c::FT_Err_Missing_Startfont_Field,
        FtError::MissingFontField => c::FT_Err_Missing_Font_Field,
        FtError::MissingSizeField => c::FT_Err_Missing_Size_Field,
        FtError::MissingFontboundingboxField => c::FT_Err_Missing_Fontboundingbox_Field,
        FtError::MissingCharsField => c::FT_Err_Missing_Chars_Field,
        FtError::MissingStartcharField => c::FT_Err_Missing_Startchar_Field,
        FtError::MissingEncodingField => c::FT_Err_Missing_Encoding_Field,
        FtError::MissingBbxField => c::FT_Err_Missing_Bbx_Field,
        FtError::BbxTooBig => c::FT_Err_Bbx_Too_Big,
        FtError::CorruptedFontHeader => c::FT_Err_Corrupted_Font_Header,
        FtError::CorruptedFontGlyphs => c::FT_Err_Corrupted_Font_Glyphs,
        FtError::UnknownFreetypeError => c::FT_Err_Max,
    }) as c::FT_Error
}

/// Compute (a*b)/0x10000 with maximum accuracy.
// PORT NOTE: `computations.mulFix`.
pub fn mul_fix(a: i32, b: i32) -> i32 {
    // c.FT_MulFix(a: FT_Long, b: FT_Long) -> FT_Long
    unsafe { c::FT_MulFix(a as c::FT_Long, b as c::FT_Long) as i32 }
}

/// FT_Tag.
// PORT NOTE: `tag.zig`'s `Tag = packed struct(u32) { d, c, b, a }`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tag {
    pub d: u8,
    pub c: u8,
    pub b: u8,
    pub a: u8,
}

impl Tag {
    pub fn init(v: &[u8; 4]) -> Tag {
        Tag {
            a: v[0],
            b: v[1],
            c: v[2],
            d: v[3],
        }
    }

    /// The `u32` bit-value of the packed struct (first field = least-significant
    /// byte): `(a<<24) | (b<<16) | (c<<8) | d`.
    // PORT NOTE: reproduces `@as(u32, @bitCast(tag))` for the Zig packed layout.
    pub fn to_u32(self) -> u32 {
        ((self.a as u32) << 24) | ((self.b as u32) << 16) | ((self.c as u32) << 8) | (self.d as u32)
    }

    /// Converts the ID to a string.
    // PORT NOTE: `tag.zig`'s `Tag.str`. The reference returns `[4]u8` BY VALUE
    // (its doc comment about "the lifetime of the self pointer" predates that);
    // the Rust return is the same owned 4-byte array — a byte array, never a
    // lossy `String` (the tag bytes are not required to be UTF-8).
    pub fn str(self) -> [u8; 4] {
        [self.a, self.b, self.c, self.d]
    }
}

/// An enumeration to specify character sets supported by charmaps. Used in the
/// FT_Select_Charmap API function.
// PORT NOTE: `face.zig`'s `Encoding = enum(u31)` — all 14 variants.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Encoding {
    None,
    MsSymbol,
    Unicode,
    Sjis,
    Prc,
    Big5,
    Wansung,
    Johab,
    AdobeStandard,
    AdobeExpert,
    AdobeCustom,
    AdobeLatin1,
    OldLatin2,
    AppleRoman,
}

impl Encoding {
    /// `@intFromEnum(encoding)` — the raw `FT_Encoding` tag value.
    pub fn to_ft(self) -> c::FT_Encoding {
        (match self {
            Encoding::None => c::FT_Encoding__FT_ENCODING_NONE,
            Encoding::MsSymbol => c::FT_Encoding__FT_ENCODING_MS_SYMBOL,
            Encoding::Unicode => c::FT_Encoding__FT_ENCODING_UNICODE,
            Encoding::Sjis => c::FT_Encoding__FT_ENCODING_SJIS,
            Encoding::Prc => c::FT_Encoding__FT_ENCODING_PRC,
            Encoding::Big5 => c::FT_Encoding__FT_ENCODING_BIG5,
            Encoding::Wansung => c::FT_Encoding__FT_ENCODING_WANSUNG,
            Encoding::Johab => c::FT_Encoding__FT_ENCODING_JOHAB,
            Encoding::AdobeStandard => c::FT_Encoding__FT_ENCODING_ADOBE_STANDARD,
            Encoding::AdobeExpert => c::FT_Encoding__FT_ENCODING_ADOBE_EXPERT,
            Encoding::AdobeCustom => c::FT_Encoding__FT_ENCODING_ADOBE_CUSTOM,
            Encoding::AdobeLatin1 => c::FT_Encoding__FT_ENCODING_ADOBE_LATIN_1,
            Encoding::OldLatin2 => c::FT_Encoding__FT_ENCODING_OLD_LATIN_2,
            Encoding::AppleRoman => c::FT_Encoding__FT_ENCODING_APPLE_ROMAN,
        }) as c::FT_Encoding
    }
}

/// <https://freetype.org/freetype2/docs/reference/ft2-glyph_retrieval.html#ft_render_mode>
// PORT NOTE: `face.zig`'s `RenderMode = enum(c_uint)` — all 6 variants.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenderMode {
    Normal,
    Light,
    Mono,
    Lcd,
    LcdV,
    Sdf,
}

impl RenderMode {
    /// `@intFromEnum(render_mode)` — the raw `FT_Render_Mode` value.
    pub fn to_ft(self) -> c::FT_Render_Mode {
        (match self {
            RenderMode::Normal => c::FT_Render_Mode__FT_RENDER_MODE_NORMAL,
            RenderMode::Light => c::FT_Render_Mode__FT_RENDER_MODE_LIGHT,
            RenderMode::Mono => c::FT_Render_Mode__FT_RENDER_MODE_MONO,
            RenderMode::Lcd => c::FT_Render_Mode__FT_RENDER_MODE_LCD,
            RenderMode::LcdV => c::FT_Render_Mode__FT_RENDER_MODE_LCD_V,
            RenderMode::Sdf => c::FT_Render_Mode__FT_RENDER_MODE_SDF,
        }) as c::FT_Render_Mode
    }
}

/// Target hinting mode for `LoadFlags`.
// PORT NOTE: `face.zig`'s `LoadFlags.Target = enum(u4)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Normal = 0,
    Light = 1,
    Mono = 2,
    Lcd = 3,
    LcdV = 4,
}

/// Flags for `FT_Load_Glyph`.
// PORT NOTE: `face.zig`'s `LoadFlags = packed struct(c_int)`. The full 32-bit layout
// is reproduced by [`LoadFlags::to_bits`]; the face only sets a subset of these,
// but the whole struct is carried for fidelity.
#[derive(Clone, Copy, Debug)]
pub struct LoadFlags {
    pub no_scale: bool,
    pub no_hinting: bool,
    pub render: bool,
    pub no_bitmap: bool,
    pub vertical_layout: bool,
    pub force_autohint: bool,
    pub crop_bitmap: bool,
    pub pedantic: bool,
    pub advance_only: bool,
    pub ignore_global_advance_width: bool,
    pub no_recurse: bool,
    pub ignore_transform: bool,
    pub monochrome: bool,
    pub linear_design: bool,
    pub sbits_only: bool,
    pub no_autohint: bool,
    pub target: Target,
    pub color: bool,
    pub compute_metrics: bool,
    pub bitmap_metrics_only: bool,
    pub svg_only: bool,
    pub no_svg: bool,
}

impl Default for LoadFlags {
    fn default() -> Self {
        LoadFlags {
            no_scale: false,
            no_hinting: false,
            render: false,
            no_bitmap: false,
            vertical_layout: false,
            force_autohint: false,
            crop_bitmap: false,
            pedantic: false,
            advance_only: false,
            ignore_global_advance_width: false,
            no_recurse: false,
            ignore_transform: false,
            monochrome: false,
            linear_design: false,
            sbits_only: false,
            no_autohint: false,
            target: Target::Normal,
            color: false,
            compute_metrics: false,
            bitmap_metrics_only: false,
            svg_only: false,
            no_svg: false,
        }
    }
}

impl LoadFlags {
    /// The `c_int` value of the packed struct — the exact FreeType `FT_LOAD_*`
    /// bit layout (LSB-first), matching the reference `@bitCast(load_flags)`.
    pub fn to_bits(self) -> c::FT_Int32 {
        let mut v: u32 = 0;
        let set = |v: &mut u32, cond: bool, bit: u32| {
            if cond {
                *v |= 1 << bit;
            }
        };
        set(&mut v, self.no_scale, 0);
        set(&mut v, self.no_hinting, 1);
        set(&mut v, self.render, 2);
        set(&mut v, self.no_bitmap, 3);
        set(&mut v, self.vertical_layout, 4);
        set(&mut v, self.force_autohint, 5);
        set(&mut v, self.crop_bitmap, 6);
        set(&mut v, self.pedantic, 7);
        set(&mut v, self.advance_only, 8);
        set(&mut v, self.ignore_global_advance_width, 9);
        set(&mut v, self.no_recurse, 10);
        set(&mut v, self.ignore_transform, 11);
        set(&mut v, self.monochrome, 12);
        set(&mut v, self.linear_design, 13);
        set(&mut v, self.sbits_only, 14);
        set(&mut v, self.no_autohint, 15);
        v |= ((self.target as u32) & 0xF) << 16;
        set(&mut v, self.color, 20);
        set(&mut v, self.compute_metrics, 21);
        set(&mut v, self.bitmap_metrics_only, 22);
        set(&mut v, self.svg_only, 23);
        set(&mut v, self.no_svg, 24);
        v as c::FT_Int32
    }

    /// The inverse of [`LoadFlags::to_bits`] — reproduces the reference
    /// `@as(LoadFlags, @bitCast(cval))` over the same LSB-first layout.
    pub fn from_bits(v: c::FT_Int32) -> LoadFlags {
        let v = v as u32;
        let get = |bit: u32| (v & (1 << bit)) != 0;
        LoadFlags {
            no_scale: get(0),
            no_hinting: get(1),
            render: get(2),
            no_bitmap: get(3),
            vertical_layout: get(4),
            force_autohint: get(5),
            crop_bitmap: get(6),
            pedantic: get(7),
            advance_only: get(8),
            ignore_global_advance_width: get(9),
            no_recurse: get(10),
            ignore_transform: get(11),
            monochrome: get(12),
            linear_design: get(13),
            sbits_only: get(14),
            no_autohint: get(15),
            target: match (v >> 16) & 0xF {
                1 => Target::Light,
                2 => Target::Mono,
                3 => Target::Lcd,
                4 => Target::LcdV,
                // The `enum(u4)` has only 5 named values; FreeType never emits
                // any other target, so anything else decodes as `.normal`.
                _ => Target::Normal,
            },
            color: get(20),
            compute_metrics: get(21),
            bitmap_metrics_only: get(22),
            svg_only: get(23),
            no_svg: get(24),
        }
    }
}

/// An enumeration to specify indices of SFNT tables loaded and parsed by
/// FreeType during initialization of an SFNT font. Used in the
/// FT_Get_Sfnt_Table API function.
// PORT NOTE: `face.zig`'s `SfntTag = enum(c_int)` — all 7 variants. Rust has no
// comptime `DataType()`; instead the [`Face`] exposes one typed accessor per tag
// the reference's `DataType()` models (`head`/`os2`/`post`/`hhea`; the rest are
// `unreachable` there).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SfntTag {
    Head,
    Maxp,
    Os2,
    Hhea,
    Vhea,
    Post,
    Pclt,
}

impl SfntTag {
    /// `@intFromEnum(tag)` — the raw `FT_Sfnt_Tag` value.
    pub fn to_ft(self) -> c::FT_Sfnt_Tag {
        (match self {
            SfntTag::Head => c::FT_Sfnt_Tag__FT_SFNT_HEAD,
            SfntTag::Maxp => c::FT_Sfnt_Tag__FT_SFNT_MAXP,
            SfntTag::Os2 => c::FT_Sfnt_Tag__FT_SFNT_OS2,
            SfntTag::Hhea => c::FT_Sfnt_Tag__FT_SFNT_HHEA,
            SfntTag::Vhea => c::FT_Sfnt_Tag__FT_SFNT_VHEA,
            SfntTag::Post => c::FT_Sfnt_Tag__FT_SFNT_POST,
            SfntTag::Pclt => c::FT_Sfnt_Tag__FT_SFNT_PCLT,
        }) as c::FT_Sfnt_Tag
    }
}

/// A FreeType library object.
// PORT NOTE: `pkg/freetype/Library.zig`.
#[derive(Clone, Copy, Debug)]
pub struct Library {
    pub handle: c::FT_Library,
}

impl Library {
    /// Initialize a new FreeType library object.
    // PORT NOTE: `Library.init`.
    pub fn init() -> Result<Library, FtError> {
        let mut handle: c::FT_Library = std::ptr::null_mut();
        int_to_error(unsafe { c::FT_Init_FreeType(&mut handle) })?;
        Ok(Library { handle })
    }

    /// Destroy a given FreeType library object and all of its children.
    // PORT NOTE: `Library.deinit`.
    pub fn deinit(self) {
        unsafe {
            c::FT_Done_FreeType(self.handle);
        }
    }

    /// Return the version of the FreeType library being used. This is useful when
    /// dynamically linking to the library, since one cannot use the macros
    /// FREETYPE_MAJOR, FREETYPE_MINOR, and FREETYPE_PATCH.
    // PORT NOTE: `Library.version`.
    pub fn version(self) -> Version {
        let mut v = Version {
            major: 0,
            minor: 0,
            patch: 0,
        };
        unsafe {
            c::FT_Library_Version(self.handle, &mut v.major, &mut v.minor, &mut v.patch);
        }
        v
    }

    /// Call FT_New_Face to open a font from a file.
    // PORT NOTE: `Library.initFace`.
    pub fn init_face(self, path: &std::ffi::CStr, index: i32) -> Result<Face, FtError> {
        let mut handle: c::FT_Face = std::ptr::null_mut();
        int_to_error(unsafe {
            c::FT_New_Face(self.handle, path.as_ptr(), index as c::FT_Long, &mut handle)
        })?;
        Ok(Face { handle })
    }

    /// Call FT_Open_Face to open a font that has been loaded into memory.
    // PORT NOTE: `Library.initMemoryFace`.
    pub fn init_memory_face(self, data: &[u8], index: i32) -> Result<Face, FtError> {
        let mut handle: c::FT_Face = std::ptr::null_mut();
        int_to_error(unsafe {
            c::FT_New_Memory_Face(
                self.handle,
                data.as_ptr(),
                data.len() as c::FT_Long,
                index as c::FT_Long,
                &mut handle,
            )
        })?;
        Ok(Face { handle })
    }

    /// Call when you're done with a loaded MM var.
    // PORT NOTE: `Library.doneMMVar`. `mm` is an opaque handle handed straight back to
    // FreeType (not dereferenced here); it must come from `Face::get_mm_var`.
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub fn done_mm_var(self, mm: *mut c::FT_MM_Var) {
        unsafe {
            c::FT_Done_MM_Var(self.handle, mm);
        }
    }
}

/// The FreeType library version triple.
// PORT NOTE: `Library.zig`'s `Version`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Version {
    pub major: i32,
    pub minor: i32,
    pub patch: i32,
}

/// `std.fmt.bufPrint` ran out of room.
// PORT NOTE: the reference's `toString` propagates `std.fmt.BufPrintError`
// (`error.NoSpaceLeft`); this is that single-atom error set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NoSpaceLeft;

impl Version {
    /// Convert the version to a string. The buffer should be able to
    /// accommodate the size, recommended to be at least 8 chars wide.
    /// The returned slice will be a slice of buf that contains the full
    /// version string.
    // PORT NOTE: `Version.toString`. Returns the written BYTE slice of `buf`
    // (never a lossy `String`), mirroring `bufPrint`'s `[]const u8`.
    #[allow(clippy::wrong_self_convention)]
    pub fn to_string(self, buf: &mut [u8]) -> Result<&[u8], NoSpaceLeft> {
        use std::io::Write;
        // return try std.fmt.bufPrint(buf, "{d}.{d}.{d}", .{ major, minor, patch });
        let mut cursor = std::io::Cursor::new(&mut buf[..]);
        write!(cursor, "{}.{}.{}", self.major, self.minor, self.patch).map_err(|_| NoSpaceLeft)?;
        let n = cursor.position() as usize;
        Ok(&buf[..n])
    }
}

/// A FreeType face object.
// PORT NOTE: `pkg/freetype/face.zig`'s `Face`.
#[derive(Clone, Copy, Debug)]
pub struct Face {
    pub handle: c::FT_Face,
}

impl Face {
    /// Decrement the reference counter, destroying the face at zero.
    // PORT NOTE: `Face.deinit`.
    pub fn deinit(self) {
        unsafe {
            c::FT_Done_Face(self.handle);
        }
    }

    /// Increment the counter of the face.
    // PORT NOTE: `Face.ref`.
    pub fn ref_(self) {
        unsafe {
            c::FT_Reference_Face(self.handle);
        }
    }

    /// True whenever a face object contains some embedded bitmaps.
    // PORT NOTE: `Face.hasFixedSizes` (`c.FT_HAS_FIXED_SIZES`).
    pub fn has_fixed_sizes(self) -> bool {
        (unsafe { (*self.handle).face_flags } & (c::FT_FACE_FLAG_FIXED_SIZES as c::FT_Long)) != 0
    }

    /// True whenever a face object contains tables for color glyphs.
    // PORT NOTE: `Face.hasColor` (`c.FT_HAS_COLOR`).
    pub fn has_color(self) -> bool {
        (unsafe { (*self.handle).face_flags } & (c::FT_FACE_FLAG_COLOR as c::FT_Long)) != 0
    }

    /// True whenever a face object contains an 'sbix' OpenType table and outline
    /// glyphs.
    // PORT NOTE: `Face.hasSBIX` (`c.FT_HAS_SBIX`).
    pub fn has_sbix(self) -> bool {
        (unsafe { (*self.handle).face_flags } & (c::FT_FACE_FLAG_SBIX as c::FT_Long)) != 0
    }

    /// True whenever a face object contains some multiple masters.
    // PORT NOTE: `Face.hasMultipleMasters` (`c.FT_HAS_MULTIPLE_MASTERS`).
    pub fn has_multiple_masters(self) -> bool {
        (unsafe { (*self.handle).face_flags } & (c::FT_FACE_FLAG_MULTIPLE_MASTERS as c::FT_Long))
            != 0
    }

    /// True whenever a face object contains a scalable font face.
    // PORT NOTE: `Face.isScalable` (`c.FT_IS_SCALABLE`).
    pub fn is_scalable(self) -> bool {
        (unsafe { (*self.handle).face_flags } & (c::FT_FACE_FLAG_SCALABLE as c::FT_Long)) != 0
    }

    /// Select a given charmap by its encoding tag.
    // PORT NOTE: `Face.selectCharmap`.
    pub fn select_charmap(self, encoding: Encoding) -> Result<(), FtError> {
        int_to_error(unsafe { c::FT_Select_Charmap(self.handle, encoding.to_ft()) })
    }

    /// Request the nominal size (in points).
    // PORT NOTE: `Face.setCharSize`.
    pub fn set_char_size(
        self,
        char_width: i32,
        char_height: i32,
        horz_resolution: u16,
        vert_resolution: u16,
    ) -> Result<(), FtError> {
        int_to_error(unsafe {
            c::FT_Set_Char_Size(
                self.handle,
                char_width as c::FT_F26Dot6,
                char_height as c::FT_F26Dot6,
                horz_resolution as c::FT_UInt,
                vert_resolution as c::FT_UInt,
            )
        })
    }

    /// Select a bitmap strike.
    // PORT NOTE: `Face.selectSize`.
    pub fn select_size(self, idx: i32) -> Result<(), FtError> {
        int_to_error(unsafe { c::FT_Select_Size(self.handle, idx as std::os::raw::c_int) })
    }

    /// Return the glyph index of a given character code, or `None`.
    // PORT NOTE: `Face.getCharIndex`.
    pub fn get_char_index(self, char: u32) -> Option<u32> {
        let i = unsafe { c::FT_Get_Char_Index(self.handle, char as c::FT_ULong) };
        if i == 0 {
            None
        } else {
            Some(i)
        }
    }

    /// Load a glyph into the glyph slot of a face object.
    // PORT NOTE: `Face.loadGlyph`.
    pub fn load_glyph(self, glyph_index: u32, load_flags: LoadFlags) -> Result<(), FtError> {
        int_to_error(unsafe {
            c::FT_Load_Glyph(self.handle, glyph_index as c::FT_UInt, load_flags.to_bits())
        })
    }

    /// Convert a given glyph image to a bitmap.
    // PORT NOTE: `Face.renderGlyph`.
    pub fn render_glyph(self, render_mode: RenderMode) -> Result<(), FtError> {
        int_to_error(unsafe { c::FT_Render_Glyph((*self.handle).glyph, render_mode.to_ft()) })
    }

    /// Retrieve the number of name strings in the SFNT 'name' table.
    // PORT NOTE: `Face.getSfntNameCount`.
    pub fn get_sfnt_name_count(self) -> usize {
        unsafe { c::FT_Get_Sfnt_Name_Count(self.handle) as usize }
    }

    /// Retrieve a string of the SFNT 'name' table for a given index.
    // PORT NOTE: `Face.getSfntName`.
    pub fn get_sfnt_name(self, i: usize) -> Result<c::FT_SfntName, FtError> {
        let mut name: c::FT_SfntName = unsafe { std::mem::zeroed() };
        int_to_error(unsafe { c::FT_Get_Sfnt_Name(self.handle, i as c::FT_UInt, &mut name) })?;
        Ok(name)
    }

    /// Return a pointer to the 'head' SFNT table, or `None`.
    // PORT NOTE: `Face.getSfntTable(.head)`.
    pub fn get_sfnt_head(self) -> Option<*mut c::TT_Header> {
        let p = unsafe { c::FT_Get_Sfnt_Table(self.handle, SfntTag::Head.to_ft()) };
        if p.is_null() {
            None
        } else {
            Some(p.cast())
        }
    }

    /// Return a pointer to the 'OS/2' SFNT table, or `None`.
    // PORT NOTE: `Face.getSfntTable(.os2)`.
    pub fn get_sfnt_os2(self) -> Option<*mut c::TT_OS2> {
        let p = unsafe { c::FT_Get_Sfnt_Table(self.handle, SfntTag::Os2.to_ft()) };
        if p.is_null() {
            None
        } else {
            Some(p.cast())
        }
    }

    /// Return a pointer to the 'post' SFNT table, or `None`.
    // PORT NOTE: `Face.getSfntTable(.post)`.
    pub fn get_sfnt_post(self) -> Option<*mut c::TT_Postscript> {
        let p = unsafe { c::FT_Get_Sfnt_Table(self.handle, SfntTag::Post.to_ft()) };
        if p.is_null() {
            None
        } else {
            Some(p.cast())
        }
    }

    /// Return a pointer to the 'hhea' SFNT table, or `None`.
    // PORT NOTE: `Face.getSfntTable(.hhea)`.
    pub fn get_sfnt_hhea(self) -> Option<*mut c::TT_HoriHeader> {
        let p = unsafe { c::FT_Get_Sfnt_Table(self.handle, SfntTag::Hhea.to_ft()) };
        if p.is_null() {
            None
        } else {
            Some(p.cast())
        }
    }

    /// Load any SFNT font table into client memory.
    // PORT NOTE: `Face.loadSfntTable`.
    pub fn load_sfnt_table(self, tag: Tag) -> Result<Option<Vec<u8>>, FtError> {
        let tag_c: std::os::raw::c_ulong = tag.to_u32() as std::os::raw::c_ulong;

        // Get the length of the table in bytes.
        let mut len: std::os::raw::c_ulong = 0;
        int_to_error(unsafe {
            c::FT_Load_Sfnt_Table(self.handle, tag_c, 0, std::ptr::null_mut(), &mut len)
        })?;

        // If our length is zero we don't have a table.
        if len == 0 {
            return Ok(None);
        }

        // Allocate a buffer to hold the table and load it.
        let mut buf: Vec<u8> = vec![0u8; len as usize];
        int_to_error(unsafe {
            c::FT_Load_Sfnt_Table(self.handle, tag_c, 0, buf.as_mut_ptr(), &mut len)
        })?;

        Ok(Some(buf))
    }

    /// Check whether a given SFNT table is available in a face.
    // PORT NOTE: `Face.hasSfntTable`.
    pub fn has_sfnt_table(self, tag: Tag) -> bool {
        // const tag_c: c_ulong = @intCast(@as(u32, @bitCast(tag)));
        let tag_c: std::os::raw::c_ulong = tag.to_u32() as std::os::raw::c_ulong;
        // var len: c_ulong = 0;
        let mut len: std::os::raw::c_ulong = 0;
        // const res = c.FT_Load_Sfnt_Table(self.handle, tag_c, 0, null, &len);
        let res =
            unsafe { c::FT_Load_Sfnt_Table(self.handle, tag_c, 0, std::ptr::null_mut(), &mut len) };
        // _ = intToError(res) catch return false;
        if int_to_error(res).is_err() {
            return false;
        }
        // return len != 0;
        len != 0
    }

    /// Retrieve the font variation descriptor for a font.
    // PORT NOTE: `Face.getMMVar`.
    pub fn get_mm_var(self) -> Result<*mut c::FT_MM_Var, FtError> {
        let mut result: *mut c::FT_MM_Var = std::ptr::null_mut();
        int_to_error(unsafe { c::FT_Get_MM_Var(self.handle, &mut result) })?;
        Ok(result)
    }

    /// Get the design coordinates of the currently selected interpolated font.
    // PORT NOTE: `Face.getVarDesignCoordinates`.
    pub fn get_var_design_coordinates(self, coords: &mut [c::FT_Fixed]) -> Result<(), FtError> {
        int_to_error(unsafe {
            c::FT_Get_Var_Design_Coordinates(
                self.handle,
                coords.len() as c::FT_UInt,
                coords.as_mut_ptr(),
            )
        })
    }

    /// Choose an interpolated font design through design coordinates.
    // PORT NOTE: `Face.setVarDesignCoordinates`.
    pub fn set_var_design_coordinates(self, coords: &mut [c::FT_Fixed]) -> Result<(), FtError> {
        int_to_error(unsafe {
            c::FT_Set_Var_Design_Coordinates(
                self.handle,
                coords.len() as c::FT_UInt,
                coords.as_mut_ptr(),
            )
        })
    }

    /// Set the transformation that is applied to glyph images when they are
    /// loaded into a glyph slot through FT_Load_Glyph.
    // PORT NOTE: `Face.setTransform`. The reference takes `?*const c.FT_Matrix` /
    // `?*const c.FT_Vector` and `@constCast`s them for the C prototype (FreeType
    // does not write through either pointer); rustty takes the same optional
    // shared references and casts them the same way.
    pub fn set_transform(self, matrix: Option<&c::FT_Matrix>, delta: Option<&c::FT_Vector>) {
        let matrix: *mut c::FT_Matrix = match matrix {
            Some(m) => (m as *const c::FT_Matrix).cast_mut(),
            None => std::ptr::null_mut(),
        };
        let delta: *mut c::FT_Vector = match delta {
            Some(d) => (d as *const c::FT_Vector).cast_mut(),
            None => std::ptr::null_mut(),
        };
        unsafe {
            c::FT_Set_Transform(self.handle, matrix, delta);
        }
    }
}

/// The test fixtures the package's own tests load.
// PORT NOTE: `pkg/freetype/test.zig` (`pub const font_regular = @embedFile(...)`).
// The reference ships the fixture at `pkg/freetype/res/FiraCode-Regular.ttf`; the
// byte-identical file is vendored here at `res/FiraCode-Regular.ttf`.
pub mod testing {
    /// `pub const font_regular = @embedFile("res/FiraCode-Regular.ttf");`
    pub const FONT_REGULAR: &[u8] = include_bytes!("../res/FiraCode-Regular.ttf");
}

// The `test { ... }` blocks of `pkg/freetype` (7 total): `errors.zig:290`
// "error conversion", `Library.zig:81` "basics", `face.zig`'s three `LoadFlags`
// tables (`:294` "bitcast", `:311` "all flags individually", `:348` "all load
// targets"), `face.zig:364` "loading memory font", and `main.zig:16`'s
// `refAllDecls`.
#[cfg(test)]
mod tests {
    use super::*;

    // test "error conversion" {
    //     try intToError(c.FT_Err_Ok);
    //     try expectError(Error.OutOfMemory, intToError(c.FT_Err_Out_Of_Memory));
    // }
    #[test]
    fn error_conversion() {
        assert_eq!(Ok(()), int_to_error(c::FT_Err_Ok as c::FT_Error));
        assert_eq!(
            Err(FtError::OutOfMemory),
            int_to_error(c::FT_Err_Out_Of_Memory as c::FT_Error)
        );

        // PORT NOTE (additive): the reference never exercises `errorToInt` (it is
        // not reachable from `main.zig`'s `refAllDecls`, so Zig never analyzes it).
        // Round-trip every mapped code so the two tables can never drift.
        for &(code, err) in &[
            (c::FT_Err_Cannot_Open_Resource, FtError::CannotOpenResource),
            (c::FT_Err_Out_Of_Memory, FtError::OutOfMemory),
            (c::FT_Err_Invalid_Argument, FtError::InvalidArgument),
            (
                c::FT_Err_Corrupted_Font_Glyphs,
                FtError::CorruptedFontGlyphs,
            ),
            (c::FT_Err_Bbx_Too_Big, FtError::BbxTooBig),
        ] {
            assert_eq!(Err(err), int_to_error(code as c::FT_Error));
            assert_eq!(code as c::FT_Error, error_to_int(err));
        }

        // An unmapped (out-of-range) code lands on the catch-all.
        assert_eq!(
            Err(FtError::UnknownFreetypeError),
            int_to_error(0x7FFF as c::FT_Error)
        );
    }

    // test "basics" {
    //     var lib = try init(); defer lib.deinit();
    //     const vsn = lib.version();
    //     try testing.expect(vsn.major > 1);
    //     var buf: [32]u8 = undefined;
    //     _ = try vsn.toString(&buf);
    // }
    #[test]
    fn basics() {
        let lib = Library::init().unwrap();

        let vsn = lib.version();
        assert!(vsn.major > 1);

        let mut buf = [0u8; 32];
        let s = vsn.to_string(&mut buf).unwrap();

        // PORT NOTE (additive): the reference discards the `toString` result; assert
        // the exact `{d}.{d}.{d}` rendering so the format cannot drift.
        assert_eq!(
            format!("{}.{}.{}", vsn.major, vsn.minor, vsn.patch).as_bytes(),
            s
        );

        lib.deinit();
    }

    // test "bitcast" (face.zig's LoadFlags)
    #[test]
    fn load_flags_bitcast() {
        // const cval: i32 = c.FT_LOAD_RENDER | c.FT_LOAD_PEDANTIC | c.FT_LOAD_COLOR;
        let cval: c::FT_Int32 =
            (c::FT_LOAD_RENDER | c::FT_LOAD_PEDANTIC | c::FT_LOAD_COLOR) as c::FT_Int32;
        let flags = LoadFlags::from_bits(cval);
        assert!(!flags.no_hinting);
        assert!(flags.render);
        assert!(flags.render);
        assert!(flags.pedantic);
        assert!(flags.color);

        // Verify bit alignment (for bit 9)
        let cval2: c::FT_Int32 = c::FT_LOAD_IGNORE_GLOBAL_ADVANCE_WIDTH as c::FT_Int32;
        let flags2 = LoadFlags::from_bits(cval2);
        assert!(flags2.ignore_global_advance_width);
        assert!(!flags2.no_recurse);
    }

    // test "all flags individually" (face.zig's LoadFlags)
    #[test]
    fn load_flags_all_flags_individually() {
        // try testing.expectEqual(c.FT_LOAD_DEFAULT, @as(c_int, @bitCast(LoadFlags{})));
        assert_eq!(
            c::FT_LOAD_DEFAULT as c::FT_Int32,
            LoadFlags::default().to_bits()
        );

        // The reference's `inline for` over (c-constant, field-name) pairs; Rust has
        // no `@field`, so each pair carries a setter closure instead.
        type Setter = fn(&mut LoadFlags);
        let table: &[(u32, Setter)] = &[
            (c::FT_LOAD_NO_SCALE, |f| f.no_scale = true),
            (c::FT_LOAD_NO_HINTING, |f| f.no_hinting = true),
            (c::FT_LOAD_RENDER, |f| f.render = true),
            (c::FT_LOAD_NO_BITMAP, |f| f.no_bitmap = true),
            (c::FT_LOAD_VERTICAL_LAYOUT, |f| f.vertical_layout = true),
            (c::FT_LOAD_FORCE_AUTOHINT, |f| f.force_autohint = true),
            (c::FT_LOAD_CROP_BITMAP, |f| f.crop_bitmap = true),
            (c::FT_LOAD_PEDANTIC, |f| f.pedantic = true),
            (c::FT_LOAD_ADVANCE_ONLY, |f| f.advance_only = true),
            (c::FT_LOAD_IGNORE_GLOBAL_ADVANCE_WIDTH, |f| {
                f.ignore_global_advance_width = true
            }),
            (c::FT_LOAD_NO_RECURSE, |f| f.no_recurse = true),
            (c::FT_LOAD_IGNORE_TRANSFORM, |f| f.ignore_transform = true),
            (c::FT_LOAD_MONOCHROME, |f| f.monochrome = true),
            (c::FT_LOAD_LINEAR_DESIGN, |f| f.linear_design = true),
            (c::FT_LOAD_SBITS_ONLY, |f| f.sbits_only = true),
            (c::FT_LOAD_NO_AUTOHINT, |f| f.no_autohint = true),
            (c::FT_LOAD_COLOR, |f| f.color = true),
            (c::FT_LOAD_COMPUTE_METRICS, |f| f.compute_metrics = true),
            (c::FT_LOAD_BITMAP_METRICS_ONLY, |f| {
                f.bitmap_metrics_only = true
            }),
            (c::FT_LOAD_SVG_ONLY, |f| f.svg_only = true),
            (c::FT_LOAD_NO_SVG, |f| f.no_svg = true),
        ];
        for &(expected, set) in table {
            let mut flags = LoadFlags::default();
            set(&mut flags);
            assert_eq!(expected as c::FT_Int32, flags.to_bits());
            // PORT NOTE (additive): the round-trip through `from_bits` pins the
            // decoder against the same table.
            assert_eq!(
                flags.to_bits(),
                LoadFlags::from_bits(flags.to_bits()).to_bits()
            );
        }
    }

    // test "all load targets" (face.zig's LoadFlags)
    #[test]
    fn load_flags_all_load_targets() {
        // PORT NOTE: `FT_LOAD_TARGET_*` are function-like C macros
        // (`FT_LOAD_TARGET_( x ) = ((FT_Int32)((x) & 15) << 16)` over the
        // `FT_RENDER_MODE_*` values), which bindgen cannot const-evaluate; they are
        // reproduced here from the same header definition.
        // (`FT_Render_Mode_`: bindgen gives the enum's constants `c_uint` on
        // macOS/Linux and `c_int` on MSVC.)
        fn ft_load_target(render_mode: c::FT_Render_Mode_) -> c::FT_Int32 {
            ((render_mode & 15) << 16) as c::FT_Int32
        }

        for &(expected, target) in &[
            (
                ft_load_target(c::FT_Render_Mode__FT_RENDER_MODE_NORMAL),
                Target::Normal,
            ),
            (
                ft_load_target(c::FT_Render_Mode__FT_RENDER_MODE_LIGHT),
                Target::Light,
            ),
            (
                ft_load_target(c::FT_Render_Mode__FT_RENDER_MODE_MONO),
                Target::Mono,
            ),
            (
                ft_load_target(c::FT_Render_Mode__FT_RENDER_MODE_LCD),
                Target::Lcd,
            ),
            (
                ft_load_target(c::FT_Render_Mode__FT_RENDER_MODE_LCD_V),
                Target::LcdV,
            ),
        ] {
            let flags = LoadFlags {
                target,
                ..Default::default()
            };
            assert_eq!(expected, flags.to_bits());
        }
    }

    // test "loading memory font" (face.zig:364)
    #[test]
    fn loading_memory_font() {
        // const font_data = @import("test.zig").font_regular;
        let font_data = testing::FONT_REGULAR;

        // var lib = try Library.init(); defer lib.deinit();
        let lib = Library::init().unwrap();
        // var face = try lib.initMemoryFace(font_data, 0); defer face.deinit();
        let face = lib.init_memory_face(font_data, 0).unwrap();

        // Try APIs
        // try face.selectCharmap(.unicode);
        face.select_charmap(Encoding::Unicode).unwrap();
        // try testing.expect(!face.hasFixedSizes());
        assert!(!face.has_fixed_sizes());
        // try face.setCharSize(12, 0, 0, 0);
        face.set_char_size(12, 0, 0, 0).unwrap();

        // Try loading
        // const idx = face.getCharIndex('A').?;
        let idx = face.get_char_index('A' as u32).unwrap();
        // try face.loadGlyph(idx, .{});
        face.load_glyph(idx, LoadFlags::default()).unwrap();

        // Try getting a truetype table
        // const os2 = face.getSfntTable(.os2);
        // try testing.expect(os2 != null);
        assert!(face.get_sfnt_os2().is_some());

        // PORT NOTE (additive): exercise the rest of the newly carved surface on the
        // same fixture — the pinned package has no test for these, but they are the
        // declarations this port previously omitted.
        assert!(face.is_scalable());
        assert!(!face.has_sbix());
        assert!(face.has_sfnt_table(Tag::init(b"OS/2")));
        assert!(!face.has_sfnt_table(Tag::init(b"zzzz")));
        assert!(face.load_sfnt_table(Tag::init(b"OS/2")).unwrap().is_some());
        // An absent table makes the length probe itself fail with `TableMissing`
        // (`loadSfntTable` propagates it — only a present-but-empty table yields
        // `null`), which is exactly what `hasSfntTable` swallows into `false`.
        assert_eq!(
            Err(FtError::TableMissing),
            face.load_sfnt_table(Tag::init(b"zzzz"))
                .map(|o| o.is_none())
        );
        // An identity transform must not disturb a subsequent render.
        let matrix = c::FT_Matrix {
            xx: 0x10000,
            xy: 0,
            yx: 0,
            yy: 0x10000,
        };
        let delta = c::FT_Vector { x: 0, y: 0 };
        face.set_transform(Some(&matrix), Some(&delta));
        face.load_glyph(idx, LoadFlags::default()).unwrap();
        face.render_glyph(RenderMode::Normal).unwrap();
        face.set_transform(None, None);

        face.deinit();
        lib.deinit();
    }

    // test { @import("std").testing.refAllDecls(@This()); } — `main.zig:16`.
    //
    // PORT NOTE: mirrors the crate's `ref_all_decls` convention (`face/mod.rs`):
    // every public declaration `main.zig` re-exports is reified so a signature
    // drift is a compile error.
    #[test]
    fn ref_all_decls() {
        let _ = core::mem::size_of::<Library>();
        let _ = core::mem::size_of::<Version>();
        let _ = core::mem::size_of::<Face>();
        let _ = core::mem::size_of::<LoadFlags>();
        let _ = core::mem::size_of::<Target>();
        let _ = core::mem::size_of::<FtError>();
        let _ = core::mem::size_of::<Error>();
        let _ = testing::FONT_REGULAR.len();

        let _ = Library::init as fn() -> Result<Library, FtError>;
        let _ = Library::deinit as fn(Library);
        let _ = Library::version as fn(Library) -> Version;
        let _ = Library::init_face as fn(Library, &std::ffi::CStr, i32) -> Result<Face, FtError>;
        let _ = Library::init_memory_face as fn(Library, &[u8], i32) -> Result<Face, FtError>;
        let _ = Library::done_mm_var as fn(Library, *mut c::FT_MM_Var);
        let _ = Version::to_string
            as for<'a> fn(Version, &'a mut [u8]) -> Result<&'a [u8], NoSpaceLeft>;

        let _ = Face::deinit as fn(Face);
        let _ = Face::ref_ as fn(Face);
        let _ = Face::has_fixed_sizes as fn(Face) -> bool;
        let _ = Face::has_color as fn(Face) -> bool;
        let _ = Face::has_sbix as fn(Face) -> bool;
        let _ = Face::has_multiple_masters as fn(Face) -> bool;
        let _ = Face::is_scalable as fn(Face) -> bool;
        let _ = Face::select_charmap as fn(Face, Encoding) -> Result<(), FtError>;
        let _ = Face::set_char_size as fn(Face, i32, i32, u16, u16) -> Result<(), FtError>;
        let _ = Face::select_size as fn(Face, i32) -> Result<(), FtError>;
        let _ = Face::get_char_index as fn(Face, u32) -> Option<u32>;
        let _ = Face::load_glyph as fn(Face, u32, LoadFlags) -> Result<(), FtError>;
        let _ = Face::render_glyph as fn(Face, RenderMode) -> Result<(), FtError>;
        let _ = Face::get_sfnt_name_count as fn(Face) -> usize;
        let _ = Face::has_sfnt_table as fn(Face, Tag) -> bool;
        let _ = Face::set_transform as fn(Face, Option<&c::FT_Matrix>, Option<&c::FT_Vector>);

        let _ = Tag::init as fn(&[u8; 4]) -> Tag;
        let _ = Tag::str as fn(Tag) -> [u8; 4];
        let _ = Tag::to_u32 as fn(Tag) -> u32;
        let _ = Encoding::to_ft as fn(Encoding) -> c::FT_Encoding;
        let _ = RenderMode::to_ft as fn(RenderMode) -> c::FT_Render_Mode;
        let _ = SfntTag::to_ft as fn(SfntTag) -> c::FT_Sfnt_Tag;
        let _ = LoadFlags::to_bits as fn(LoadFlags) -> c::FT_Int32;
        let _ = LoadFlags::from_bits as fn(c::FT_Int32) -> LoadFlags;
        let _ = int_to_error as fn(c::FT_Error) -> Result<(), FtError>;
        let _ = error_to_int as fn(FtError) -> c::FT_Error;
        let _ = mul_fix as fn(i32, i32) -> i32;

        // Every `SfntTag` maps to a distinct raw value (the 7-variant table).
        let tags = [
            SfntTag::Head,
            SfntTag::Maxp,
            SfntTag::Os2,
            SfntTag::Hhea,
            SfntTag::Vhea,
            SfntTag::Post,
            SfntTag::Pclt,
        ];
        for (i, a) in tags.iter().enumerate() {
            for b in &tags[i + 1..] {
                assert_ne!(a.to_ft(), b.to_ft());
            }
        }

        // `Tag.str` is the inverse of `Tag.init`.
        assert_eq!(*b"OS/2", Tag::init(b"OS/2").str());
    }
}
