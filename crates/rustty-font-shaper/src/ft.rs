//! Thin, faithful FFI wrappers around `freetype-sys` used by the FreeType face.
//!
//! PORT NOTE (glue): the reference imports the `freetype` package
//! (`pkg/freetype/{Library,face,tag,computations,errors,c}.zig`) — a hand-written
//! Zig wrapper over the FreeType C API. That package is its own port target with
//! its own test contract; it is not yet carved. Until it is, this module provides
//! the MINIMAL wrapper surface that `face/freetype.zig` actually calls
//! (`freetype.Library`, `freetype.Face`, `freetype.LoadFlags`, `freetype.mulFix`,
//! `freetype.Tag`, `freetype.c.*`), implemented as `unsafe` wrappers over
//! `freetype-sys` (R1: additive glue around C FFI — nothing from the ported file
//! is lost; the full `pkg/freetype` port, with its own tests, lands separately).
//!
//! The FreeType error set (`errors.zig`, 200+ variants) is represented here by a
//! single opaque [`FtError`] carrying the raw `FT_Error` code, because
//! `face/freetype.zig` only ever propagates these errors (`try`) — it never
//! matches on a specific variant. The exhaustive code→variant table is part of
//! the `pkg/freetype/errors.zig` port.

use freetype_sys as c;

/// Opaque FreeType error carrying the raw `FT_Error` code.
// PORT NOTE: stands in for `pkg/freetype/errors.zig`'s `Error` set (see module
// note). `code` is always non-zero (zero is `FT_Err_Ok`, mapped to `Ok`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FtError {
    pub code: c::FT_Error,
}

/// Map a raw `FT_Error` to `Result`: `FT_Err_Ok` (0) → `Ok`, else `Err`.
// PORT: `errors.intToError`.
#[inline]
pub fn int_to_error(err: c::FT_Error) -> Result<(), FtError> {
    if err == 0 {
        Ok(())
    } else {
        Err(FtError { code: err })
    }
}

/// Compute (a*b)/0x10000 with maximum accuracy.
// PORT: `computations.mulFix`.
pub fn mul_fix(a: i32, b: i32) -> i32 {
    // c.FT_MulFix(a: FT_Long, b: FT_Long) -> FT_Long
    unsafe { c::FT_MulFix(a as c::FT_Long, b as c::FT_Long) as i32 }
}

/// FT_Tag.
// PORT: `tag.zig`'s `Tag = packed struct(u32) { d, c, b, a }`.
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
}

/// Character-set encoding tags for `FT_Select_Charmap`.
// PORT: `face.zig`'s `Encoding` (only the variants the face uses are carved).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Encoding {
    Unicode,
}

impl Encoding {
    fn to_ft(self) -> c::FT_Encoding {
        match self {
            Encoding::Unicode => c::FT_Encoding__FT_ENCODING_UNICODE as c::FT_Encoding,
        }
    }
}

/// Render modes for `FT_Render_Glyph`.
// PORT: `face.zig`'s `RenderMode` (only `normal`/`mono` are carved — the only
// modes the face requests).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenderMode {
    Normal,
    Mono,
}

impl RenderMode {
    fn to_ft(self) -> c::FT_Render_Mode {
        match self {
            RenderMode::Normal => c::FT_Render_Mode__FT_RENDER_MODE_NORMAL as c::FT_Render_Mode,
            RenderMode::Mono => c::FT_Render_Mode__FT_RENDER_MODE_MONO as c::FT_Render_Mode,
        }
    }
}

/// Target hinting mode for `LoadFlags`.
// PORT: `face.zig`'s `LoadFlags.Target = enum(u4)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Normal = 0,
    Light = 1,
    Mono = 2,
    Lcd = 3,
    LcdV = 4,
}

/// Flags for `FT_Load_Glyph`.
// PORT: `face.zig`'s `LoadFlags = packed struct(c_int)`. The full 32-bit layout
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
}

/// SFNT table tags for `FT_Get_Sfnt_Table`.
// PORT: `face.zig`'s `SfntTag`. Rust has no comptime `DataType()`; instead the
// [`Face`] exposes one typed accessor per tag the face reads.
#[derive(Clone, Copy, Debug)]
pub enum SfntTag {
    Head,
    Os2,
    Post,
    Hhea,
}

impl SfntTag {
    fn to_ft(self) -> c::FT_Sfnt_Tag {
        (match self {
            SfntTag::Head => c::FT_Sfnt_Tag__FT_SFNT_HEAD,
            SfntTag::Os2 => c::FT_Sfnt_Tag__FT_SFNT_OS2,
            SfntTag::Post => c::FT_Sfnt_Tag__FT_SFNT_POST,
            SfntTag::Hhea => c::FT_Sfnt_Tag__FT_SFNT_HHEA,
        }) as c::FT_Sfnt_Tag
    }
}

/// A FreeType library object.
// PORT: `pkg/freetype/Library.zig`.
#[derive(Clone, Copy, Debug)]
pub struct Library {
    pub handle: c::FT_Library,
}

impl Library {
    /// Initialize a new FreeType library object.
    // PORT: `Library.init`.
    pub fn init() -> Result<Library, FtError> {
        let mut handle: c::FT_Library = std::ptr::null_mut();
        int_to_error(unsafe { c::FT_Init_FreeType(&mut handle) })?;
        Ok(Library { handle })
    }

    /// Destroy a given FreeType library object and all of its children.
    // PORT: `Library.deinit`.
    pub fn deinit(self) {
        unsafe {
            c::FT_Done_FreeType(self.handle);
        }
    }

    /// Call FT_New_Face to open a font from a file.
    // PORT: `Library.initFace`.
    pub fn init_face(self, path: &std::ffi::CStr, index: i32) -> Result<Face, FtError> {
        let mut handle: c::FT_Face = std::ptr::null_mut();
        int_to_error(unsafe {
            c::FT_New_Face(self.handle, path.as_ptr(), index as c::FT_Long, &mut handle)
        })?;
        Ok(Face { handle })
    }

    /// Call FT_Open_Face to open a font that has been loaded into memory.
    // PORT: `Library.initMemoryFace`.
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
    // PORT: `Library.doneMMVar`. `mm` is an opaque handle handed straight back to
    // FreeType (not dereferenced here); it must come from `Face::get_mm_var`.
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub fn done_mm_var(self, mm: *mut c::FT_MM_Var) {
        unsafe {
            c::FT_Done_MM_Var(self.handle, mm);
        }
    }
}

/// A FreeType face object.
// PORT: `pkg/freetype/face.zig`'s `Face`.
#[derive(Clone, Copy, Debug)]
pub struct Face {
    pub handle: c::FT_Face,
}

impl Face {
    /// Decrement the reference counter, destroying the face at zero.
    // PORT: `Face.deinit`.
    pub fn deinit(self) {
        unsafe {
            c::FT_Done_Face(self.handle);
        }
    }

    /// Increment the counter of the face.
    // PORT: `Face.ref`.
    pub fn ref_(self) {
        unsafe {
            c::FT_Reference_Face(self.handle);
        }
    }

    /// True whenever a face object contains some embedded bitmaps.
    // PORT: `Face.hasFixedSizes` (`c.FT_HAS_FIXED_SIZES`).
    pub fn has_fixed_sizes(self) -> bool {
        (unsafe { (*self.handle).face_flags } & (c::FT_FACE_FLAG_FIXED_SIZES as c::FT_Long)) != 0
    }

    /// True whenever a face object contains tables for color glyphs.
    // PORT: `Face.hasColor` (`c.FT_HAS_COLOR`).
    pub fn has_color(self) -> bool {
        (unsafe { (*self.handle).face_flags } & (c::FT_FACE_FLAG_COLOR as c::FT_Long)) != 0
    }

    /// True whenever a face object contains some multiple masters.
    // PORT: `Face.hasMultipleMasters` (`c.FT_HAS_MULTIPLE_MASTERS`).
    pub fn has_multiple_masters(self) -> bool {
        (unsafe { (*self.handle).face_flags } & (c::FT_FACE_FLAG_MULTIPLE_MASTERS as c::FT_Long))
            != 0
    }

    /// True whenever a face object contains a scalable font face.
    // PORT: `Face.isScalable` (`c.FT_IS_SCALABLE`).
    pub fn is_scalable(self) -> bool {
        (unsafe { (*self.handle).face_flags } & (c::FT_FACE_FLAG_SCALABLE as c::FT_Long)) != 0
    }

    /// Select a given charmap by its encoding tag.
    // PORT: `Face.selectCharmap`.
    pub fn select_charmap(self, encoding: Encoding) -> Result<(), FtError> {
        int_to_error(unsafe { c::FT_Select_Charmap(self.handle, encoding.to_ft()) })
    }

    /// Request the nominal size (in points).
    // PORT: `Face.setCharSize`.
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
    // PORT: `Face.selectSize`.
    pub fn select_size(self, idx: i32) -> Result<(), FtError> {
        int_to_error(unsafe { c::FT_Select_Size(self.handle, idx as std::os::raw::c_int) })
    }

    /// Return the glyph index of a given character code, or `None`.
    // PORT: `Face.getCharIndex`.
    pub fn get_char_index(self, char: u32) -> Option<u32> {
        let i = unsafe { c::FT_Get_Char_Index(self.handle, char as c::FT_ULong) };
        if i == 0 {
            None
        } else {
            Some(i)
        }
    }

    /// Load a glyph into the glyph slot of a face object.
    // PORT: `Face.loadGlyph`.
    pub fn load_glyph(self, glyph_index: u32, load_flags: LoadFlags) -> Result<(), FtError> {
        int_to_error(unsafe {
            c::FT_Load_Glyph(self.handle, glyph_index as c::FT_UInt, load_flags.to_bits())
        })
    }

    /// Convert a given glyph image to a bitmap.
    // PORT: `Face.renderGlyph`.
    pub fn render_glyph(self, render_mode: RenderMode) -> Result<(), FtError> {
        int_to_error(unsafe { c::FT_Render_Glyph((*self.handle).glyph, render_mode.to_ft()) })
    }

    /// Retrieve the number of name strings in the SFNT 'name' table.
    // PORT: `Face.getSfntNameCount`.
    pub fn get_sfnt_name_count(self) -> usize {
        unsafe { c::FT_Get_Sfnt_Name_Count(self.handle) as usize }
    }

    /// Retrieve a string of the SFNT 'name' table for a given index.
    // PORT: `Face.getSfntName`.
    pub fn get_sfnt_name(self, i: usize) -> Result<c::FT_SfntName, FtError> {
        let mut name: c::FT_SfntName = unsafe { std::mem::zeroed() };
        int_to_error(unsafe { c::FT_Get_Sfnt_Name(self.handle, i as c::FT_UInt, &mut name) })?;
        Ok(name)
    }

    /// Return a pointer to the 'head' SFNT table, or `None`.
    // PORT: `Face.getSfntTable(.head)`.
    pub fn get_sfnt_head(self) -> Option<*mut c::TT_Header> {
        let p = unsafe { c::FT_Get_Sfnt_Table(self.handle, SfntTag::Head.to_ft()) };
        if p.is_null() {
            None
        } else {
            Some(p.cast())
        }
    }

    /// Return a pointer to the 'OS/2' SFNT table, or `None`.
    // PORT: `Face.getSfntTable(.os2)`.
    pub fn get_sfnt_os2(self) -> Option<*mut c::TT_OS2> {
        let p = unsafe { c::FT_Get_Sfnt_Table(self.handle, SfntTag::Os2.to_ft()) };
        if p.is_null() {
            None
        } else {
            Some(p.cast())
        }
    }

    /// Return a pointer to the 'post' SFNT table, or `None`.
    // PORT: `Face.getSfntTable(.post)`.
    pub fn get_sfnt_post(self) -> Option<*mut c::TT_Postscript> {
        let p = unsafe { c::FT_Get_Sfnt_Table(self.handle, SfntTag::Post.to_ft()) };
        if p.is_null() {
            None
        } else {
            Some(p.cast())
        }
    }

    /// Return a pointer to the 'hhea' SFNT table, or `None`.
    // PORT: `Face.getSfntTable(.hhea)`.
    pub fn get_sfnt_hhea(self) -> Option<*mut c::TT_HoriHeader> {
        let p = unsafe { c::FT_Get_Sfnt_Table(self.handle, SfntTag::Hhea.to_ft()) };
        if p.is_null() {
            None
        } else {
            Some(p.cast())
        }
    }

    /// Load any SFNT font table into client memory.
    // PORT: `Face.loadSfntTable`.
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

    /// Retrieve the font variation descriptor for a font.
    // PORT: `Face.getMMVar`.
    pub fn get_mm_var(self) -> Result<*mut c::FT_MM_Var, FtError> {
        let mut result: *mut c::FT_MM_Var = std::ptr::null_mut();
        int_to_error(unsafe { c::FT_Get_MM_Var(self.handle, &mut result) })?;
        Ok(result)
    }

    /// Get the design coordinates of the currently selected interpolated font.
    // PORT: `Face.getVarDesignCoordinates`.
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
    // PORT: `Face.setVarDesignCoordinates`.
    pub fn set_var_design_coordinates(self, coords: &mut [c::FT_Fixed]) -> Result<(), FtError> {
        int_to_error(unsafe {
            c::FT_Set_Var_Design_Coordinates(
                self.handle,
                coords.len() as c::FT_UInt,
                coords.as_mut_ptr(),
            )
        })
    }
}
