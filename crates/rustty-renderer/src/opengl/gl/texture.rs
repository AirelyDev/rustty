//! PORTS: ghostty `pkg/opengl/Texture.zig` (221 loc), in full.
//!
//! PORT NOTE (`glad.context.<Fn>.?(…)`): see `draw.rs`'s module note — the
//! reference force-unwraps every glad slot, the `gl` crate panics identically on
//! an unloaded entry point, so the calls go straight through.
//!
//! PORT NOTE (by-value `self`): the reference's methods take `v: Texture` /
//! `b: Binding` BY VALUE — both are plain structs no wider than a `GLuint`.
//! [`Texture`] and [`Binding`] are therefore `Copy` and their methods take `self`
//! by value, so `tex.bind(…)` followed by `tex.destroy()` compiles here exactly as
//! it does there.

use std::ffi::c_void;

use gl::types::{GLenum, GLint, GLsizei, GLuint};

use super::errors;
use super::glad;

/// PORTS: `Texture.zig:1-8` — `const Texture = @This(); … id: c.GLuint`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Texture {
    pub id: GLuint,
}

impl Texture {
    /// PORTS: `Texture.zig:10-13`.
    ///
    /// ```zig
    /// pub fn active(index: c_uint) errors.Error!void {
    ///     glad.context.ActiveTexture.?(index + c.GL_TEXTURE0);
    ///     try errors.getError();
    /// }
    /// ```
    ///
    /// PORT NOTE: `index` is a texture UNIT ORDINAL, not a `GL_TEXTURE*` enum —
    /// the reference adds `GL_TEXTURE0` itself, so its call sites pass `0`, `1`, …
    /// (`renderer/opengl/RenderPass.zig`). That addition is ported verbatim rather
    /// than pushed onto callers.
    ///
    /// PORT NOTE (`index + c.GL_TEXTURE0`): Zig's `+` traps on overflow here;
    /// `wrapping_add` would silently wrap, so the plain `+` is kept — it panics in
    /// debug exactly as the reference traps, and a unit ordinal within `u32::MAX -
    /// GL_TEXTURE0` is the same precondition the reference already assumes.
    pub fn active(index: GLuint) -> Result<(), errors::Error> {
        // SAFETY: `glActiveTexture` takes an enum by value. An out-of-range unit
        // is GL_INVALID_ENUM, collected below — never UB.
        glad::context(|gl| unsafe { gl.ActiveTexture(index + gl::TEXTURE0) });
        errors::get_error()
    }

    /// Create a single texture.
    //
    // PORTS: `Texture.zig:15-21`.
    //
    // ```zig
    // pub fn create() errors.Error!Texture {
    //     var id: c.GLuint = undefined;
    //     glad.context.GenTextures.?(1, &id);
    //     try errors.getError();
    //     return .{ .id = id };
    // }
    // ```
    //
    // PORT NOTE (`var id: c.GLuint = undefined`): Rust has no safe `undefined`, so
    // the slot is zero-initialized. Behaviourally identical — `glGenTextures`
    // unconditionally overwrites it — and not observable, since the value is read
    // only after the call. Same handling as `buffer::Buffer::create`.
    //
    // PORT NOTE: unlike `Buffer.create`, this one DOES collect the error, so its
    // `errors.Error!Texture` is a genuinely inhabited error set.
    pub fn create() -> Result<Self, errors::Error> {
        let mut id: GLuint = 0;
        // SAFETY: `glGenTextures` writes exactly `n` (= 1) GLuints through the
        // pointer; `&mut id` is one live, aligned GLuint.
        glad::context(|gl| unsafe { gl.GenTextures(1, &mut id) });
        errors::get_error()?;
        Ok(Texture { id })
    }

    /// glBindTexture
    //
    // PORTS: `Texture.zig:23-28`.
    //
    // ```zig
    // pub fn bind(v: Texture, target: Target) !Binding {
    //     glad.context.BindTexture.?(@intFromEnum(target), v.id);
    //     try errors.getError();
    //     return .{ .target = target };
    // }
    // ```
    //
    // PORT NOTE: `Binding` carries ONLY the target — not the id. That is the
    // reference's own shape (`Texture.zig:127-128`), and it is why `Binding` has
    // no `destroy`: the id lives on the `Texture` the caller still holds.
    pub fn bind(self, target: Target) -> Result<Binding, errors::Error> {
        // SAFETY: binds a name this `Texture` owns to a GL target. A name bound to
        // an incompatible target is GL_INVALID_OPERATION, collected below.
        glad::context(|gl| unsafe { gl.BindTexture(target as GLenum, self.id) });
        errors::get_error()?;
        Ok(Binding { target })
    }

    /// PORTS: `Texture.zig:30-32`.
    ///
    /// ```zig
    /// pub fn destroy(v: Texture) void {
    ///     glad.context.DeleteTextures.?(1, &v.id);
    /// }
    /// ```
    ///
    /// PORT NOTE: takes `self` BY VALUE, mirroring the reference — this is NOT a
    /// `Drop` impl. See `buffer::Buffer::destroy`'s note: the reference manages GL
    /// object lifetimes explicitly, and `Texture` is `Copy`, so an RAII `Drop`
    /// would be both a deviation and unsound (a copy would double-delete).
    pub fn destroy(self) {
        // SAFETY: `glDeleteTextures` reads exactly `n` (= 1) GLuints through the
        // pointer; `&self.id` is one live, aligned GLuint. Deleting an already-
        // deleted or unknown name is silently ignored by GL, not UB.
        glad::context(|gl| unsafe { gl.DeleteTextures(1, &self.id) });
    }
}

/// Enum for possible texture binding targets.
//
// PORTS: `Texture.zig:34-46`.
//
// ```zig
// pub const Target = enum(c_uint) {
//     @"1D" = c.GL_TEXTURE_1D,
//     @"2D" = c.GL_TEXTURE_2D,
//     @"3D" = c.GL_TEXTURE_3D,
//     @"1DArray" = c.GL_TEXTURE_1D_ARRAY,
//     @"2DArray" = c.GL_TEXTURE_2D_ARRAY,
//     Rectangle = c.GL_TEXTURE_RECTANGLE,
//     CubeMap = c.GL_TEXTURE_CUBE_MAP,
//     Buffer = c.GL_TEXTURE_BUFFER,
//     @"2DMultisample" = c.GL_TEXTURE_2D_MULTISAMPLE,
//     @"2DMultisampleArray" = c.GL_TEXTURE_2D_MULTISAMPLE_ARRAY,
// };
// ```
//
// PORT NOTE (`enum(c_uint)` -> `#[repr(u32)]`): NO trailing `_`, i.e. CLOSED — so
// a real Rust enum is the exact model, as with `primitives::Primitive` and unlike
// `buffer::Target`'s open-enum newtype.
//
// PORT NOTE (the variant NAMES): the reference reaches for Zig's `@"…"` quoted
// identifiers to name five variants after the digit-leading GL suffix (`@"2D"` =
// `GL_TEXTURE_2D`). Rust has no quoted-identifier escape and forbids a leading
// digit, so those five — and ONLY those five — are prefixed to match the GL
// constant they carry (`Texture2D` = `GL_TEXTURE_2D`). The reference's own
// non-digit spellings (`Rectangle`, `CubeMap`, `Buffer`) are kept VERBATIM, which
// preserves its inconsistency rather than "tidying" it into a uniform prefix.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum Target {
    Texture1D = gl::TEXTURE_1D,
    Texture2D = gl::TEXTURE_2D,
    Texture3D = gl::TEXTURE_3D,
    Texture1DArray = gl::TEXTURE_1D_ARRAY,
    Texture2DArray = gl::TEXTURE_2D_ARRAY,
    Rectangle = gl::TEXTURE_RECTANGLE,
    CubeMap = gl::TEXTURE_CUBE_MAP,
    Buffer = gl::TEXTURE_BUFFER,
    Texture2DMultisample = gl::TEXTURE_2D_MULTISAMPLE,
    Texture2DMultisampleArray = gl::TEXTURE_2D_MULTISAMPLE_ARRAY,
}

/// Enum for possible texture parameters.
//
// PORTS: `Texture.zig:48-66`.
//
// ```zig
// pub const Parameter = enum(c_uint) {
//     BaseLevel = c.GL_TEXTURE_BASE_LEVEL,
//     CompareFunc = c.GL_TEXTURE_COMPARE_FUNC,
//     CompareMode = c.GL_TEXTURE_COMPARE_MODE,
//     LodBias = c.GL_TEXTURE_LOD_BIAS,
//     MinFilter = c.GL_TEXTURE_MIN_FILTER,
//     MagFilter = c.GL_TEXTURE_MAG_FILTER,
//     MinLod = c.GL_TEXTURE_MIN_LOD,
//     MaxLod = c.GL_TEXTURE_MAX_LOD,
//     MaxLevel = c.GL_TEXTURE_MAX_LEVEL,
//     SwizzleR = c.GL_TEXTURE_SWIZZLE_R,
//     SwizzleG = c.GL_TEXTURE_SWIZZLE_G,
//     SwizzleB = c.GL_TEXTURE_SWIZZLE_B,
//     SwizzleA = c.GL_TEXTURE_SWIZZLE_A,
//     WrapS = c.GL_TEXTURE_WRAP_S,
//     WrapT = c.GL_TEXTURE_WRAP_T,
//     WrapR = c.GL_TEXTURE_WRAP_R,
// };
// ```
//
// PORT NOTE: CLOSED (no trailing `_`) -> a real Rust enum. `adf161473` renamed
// the reference's variants to snake_case (`.min_filter`, `.wrap_s`, …); Rust
// variants stay PascalCase, so only this comment changed.
//
// PORT NOTE: shared with `Sampler.parameter` (`Sampler.zig:27`), which takes this
// very enum — hence it lives here, where the reference puts it, rather than being
// duplicated per object.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum Parameter {
    BaseLevel = gl::TEXTURE_BASE_LEVEL,
    CompareFunc = gl::TEXTURE_COMPARE_FUNC,
    CompareMode = gl::TEXTURE_COMPARE_MODE,
    LodBias = gl::TEXTURE_LOD_BIAS,
    MinFilter = gl::TEXTURE_MIN_FILTER,
    MagFilter = gl::TEXTURE_MAG_FILTER,
    MinLod = gl::TEXTURE_MIN_LOD,
    MaxLod = gl::TEXTURE_MAX_LOD,
    MaxLevel = gl::TEXTURE_MAX_LEVEL,
    SwizzleR = gl::TEXTURE_SWIZZLE_R,
    SwizzleG = gl::TEXTURE_SWIZZLE_G,
    SwizzleB = gl::TEXTURE_SWIZZLE_B,
    SwizzleA = gl::TEXTURE_SWIZZLE_A,
    WrapS = gl::TEXTURE_WRAP_S,
    WrapT = gl::TEXTURE_WRAP_T,
    WrapR = gl::TEXTURE_WRAP_R,
}

impl Parameter {
    /// The value type a parameter takes.
    ///
    /// PORTS: `Texture.zig:67-74` (`adf161473`).
    ///
    /// ```zig
    /// pub fn Type(comptime self: Parameter) type {
    ///     return switch (self) {
    ///         .min_filter => MinFilter,
    ///         .mag_filter => MagFilter,
    ///         .wrap_s, .wrap_t, .wrap_r => Wrap,
    ///         else => c.GLint,
    ///     };
    /// }
    /// ```
    ///
    /// PORT NOTE: a Zig fn returning a `type` has no Rust equivalent over a
    /// runtime enum value, so the type is named by [`ParameterType`] and each
    /// value type declares its own through [`ParameterValue::TYPE`]; `parameter`
    /// asserts the two agree where the reference's compiler checks it.
    pub const fn type_(self) -> ParameterType {
        match self {
            Self::MinFilter => ParameterType::MinFilter,
            Self::MagFilter => ParameterType::MagFilter,
            Self::WrapS | Self::WrapT | Self::WrapR => ParameterType::Wrap,
            _ => ParameterType::GLint,
        }
    }
}

/// The types [`Parameter::type_`] names. RUSTTY-ONLY: the value of the
/// reference's `Parameter.Type()`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ParameterType {
    /// `c.GLint`
    GLint,
    /// [`MinFilter`]
    MinFilter,
    /// [`MagFilter`]
    MagFilter,
    /// [`Wrap`]
    Wrap,
}

/// A value `Binding::parameter`/`Sampler::parameter` accept: the reference's
/// `value: name.Type()`, as either a `c.GLint` or one of the `enum(c_int)`
/// parameter enums, which it passes as `@intFromEnum(value)`.
pub trait ParameterValue: Copy {
    /// Which [`ParameterType`] this is.
    const TYPE: ParameterType;
    /// The value as the `GLint` `glTexParameteri`/`glSamplerParameteri` take
    /// (`@intFromEnum` for the enums).
    fn to_glint(self) -> GLint;
}

impl ParameterValue for GLint {
    const TYPE: ParameterType = ParameterType::GLint;
    fn to_glint(self) -> GLint {
        self
    }
}

impl ParameterValue for MinFilter {
    const TYPE: ParameterType = ParameterType::MinFilter;
    fn to_glint(self) -> GLint {
        self as GLint
    }
}

impl ParameterValue for MagFilter {
    const TYPE: ParameterType = ParameterType::MagFilter;
    fn to_glint(self) -> GLint {
        self as GLint
    }
}

impl ParameterValue for Wrap {
    const TYPE: ParameterType = ParameterType::Wrap;
    fn to_glint(self) -> GLint {
        self as GLint
    }
}

/// Internal format enum for texture images.
//
// PORTS: `Texture.zig:68-82`.
//
// ```zig
// pub const InternalFormat = enum(c_int) {
//     red = c.GL_RED,
//     rgb = c.GL_RGB8,
//     rgba = c.GL_RGBA8,
//
//     srgb = c.GL_SRGB8,
//     srgba = c.GL_SRGB8_ALPHA8,
//
//     rgba_compressed = c.GL_COMPRESSED_RGBA_BPTC_UNORM,
//     srgba_compressed = c.GL_COMPRESSED_SRGB_ALPHA_BPTC_UNORM,
//
//     // There are so many more that I haven't filled in.
//     _,
// };
// ```
//
// PORT NOTE (the trailing `_`): a Zig NON-EXHAUSTIVE enum — any `c_int` is a valid
// value, the seven named ones are merely the convenient spellings. Ported as a
// newtype over the tag type with the named values as associated constants, which
// is this tree's established handling (see `buffer::Target`).
//
// PORT NOTE (`rgb` = `GL_RGB8`, NOT `GL_RGB`): the reference's `InternalFormat` and
// `Format` deliberately DIFFER on the `rgb`/`rgba` spellings — sized internal
// formats here, unsized pixel formats there. Carried over exactly; "fixing" the
// asymmetry would change what `image2D` asks the driver to allocate.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct InternalFormat(pub GLint);

impl InternalFormat {
    pub const RED: Self = Self(gl::RED as GLint);
    pub const RGB: Self = Self(gl::RGB8 as GLint);
    pub const RGBA: Self = Self(gl::RGBA8 as GLint);

    pub const SRGB: Self = Self(gl::SRGB8 as GLint);
    pub const SRGBA: Self = Self(gl::SRGB8_ALPHA8 as GLint);

    pub const RGBA_COMPRESSED: Self = Self(gl::COMPRESSED_RGBA_BPTC_UNORM as GLint);
    pub const SRGBA_COMPRESSED: Self = Self(gl::COMPRESSED_SRGB_ALPHA_BPTC_UNORM as GLint);

    // There are so many more that I haven't filled in.
}

/// Format for texture images
//
// PORTS: `Texture.zig:84-93`.
//
// ```zig
// pub const Format = enum(c_uint) {
//     red = c.GL_RED,
//     rgb = c.GL_RGB,
//     rgba = c.GL_RGBA,
//     bgra = c.GL_BGRA,
//
//     // There are so many more that I haven't filled in.
//     _,
// };
// ```
//
// PORT NOTE (the trailing `_`): see [`InternalFormat`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Format(pub GLenum);

impl Format {
    pub const RED: Self = Self(gl::RED);
    pub const RGB: Self = Self(gl::RGB);
    pub const RGBA: Self = Self(gl::RGBA);
    pub const BGRA: Self = Self(gl::BGRA);

    // There are so many more that I haven't filled in.
}

/// Minification filter for textures.
//
// PORTS: `Texture.zig:95-103`.
//
// ```zig
// pub const MinFilter = enum(c_int) {
//     nearest = c.GL_NEAREST,
//     linear = c.GL_LINEAR,
//     nearest_mipmap_nearest = c.GL_NEAREST_MIPMAP_NEAREST,
//     linear_mipmap_nearest = c.GL_LINEAR_MIPMAP_NEAREST,
//     nearest_mipmap_linear = c.GL_NEAREST_MIPMAP_LINEAR,
//     linear_mipmap_linear = c.GL_LINEAR_MIPMAP_LINEAR,
// };
// ```
//
// PORT NOTE: CLOSED -> a real Rust enum, `#[repr(i32)]` for `enum(c_int)`.
//
// PORT NOTE (why an `enum(c_int)`): these feed `parameter`, which since
// `adf161473` takes them typed (`Parameter.Type()`) and passes
// `@intFromEnum(value)`; here that is [`ParameterValue::to_glint`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(i32)]
pub enum MinFilter {
    Nearest = gl::NEAREST as i32,
    Linear = gl::LINEAR as i32,
    NearestMipmapNearest = gl::NEAREST_MIPMAP_NEAREST as i32,
    LinearMipmapNearest = gl::LINEAR_MIPMAP_NEAREST as i32,
    NearestMipmapLinear = gl::NEAREST_MIPMAP_LINEAR as i32,
    LinearMipmapLinear = gl::LINEAR_MIPMAP_LINEAR as i32,
}

/// Magnification filter for textures.
//
// PORTS: `Texture.zig:105-109`.
//
// ```zig
// pub const MagFilter = enum(c_int) {
//     nearest = c.GL_NEAREST,
//     linear = c.GL_LINEAR,
// };
// ```
//
// PORT NOTE: a SEPARATE enum from [`MinFilter`] with two identically-valued
// variants — that is the reference's own modelling (GL rejects the mipmap filters
// for `GL_TEXTURE_MAG_FILTER`), not a duplication to be collapsed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(i32)]
pub enum MagFilter {
    Nearest = gl::NEAREST as i32,
    Linear = gl::LINEAR as i32,
}

/// Texture coordinate wrapping mode.
//
// PORTS: `Texture.zig:111-117`.
//
// ```zig
// pub const Wrap = enum(c_int) {
//     clamp_to_edge = c.GL_CLAMP_TO_EDGE,
//     clamp_to_border = c.GL_CLAMP_TO_BORDER,
//     mirrored_repeat = c.GL_MIRRORED_REPEAT,
//     repeat = c.GL_REPEAT,
// };
// ```
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(i32)]
pub enum Wrap {
    ClampToEdge = gl::CLAMP_TO_EDGE as i32,
    ClampToBorder = gl::CLAMP_TO_BORDER as i32,
    MirroredRepeat = gl::MIRRORED_REPEAT as i32,
    Repeat = gl::REPEAT as i32,
}

/// Data type for texture images.
//
// PORTS: `Texture.zig:119-125`.
//
// ```zig
// pub const DataType = enum(c_uint) {
//     UnsignedByte = c.GL_UNSIGNED_BYTE,
//
//     // There are so many more that I haven't filled in.
//     _,
// };
// ```
//
// PORT NOTE (the trailing `_`): see [`InternalFormat`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct DataType(pub GLenum);

impl DataType {
    pub const UNSIGNED_BYTE: Self = Self(gl::UNSIGNED_BYTE);

    // There are so many more that I haven't filled in.
}

/// PORTS: `Texture.zig:127-221` — `pub const Binding = struct { target: Target, … }`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Binding {
    pub target: Target,
}

impl Binding {
    /// PORTS: `Texture.zig:130-132`.
    ///
    /// ```zig
    /// pub fn unbind(b: *const Binding) void {
    ///     glad.context.BindTexture.?(@intFromEnum(b.target), 0);
    /// }
    /// ```
    ///
    /// PORT NOTE (`b: *const Binding`): the ONLY method in this file taking the
    /// receiver by POINTER rather than by value — every sibling below takes
    /// `b: Binding`. On a `Copy` struct with no interior mutability a `*const`
    /// receiver is just a read-only borrow, so `self`-by-value is behaviourally
    /// identical and keeps the receiver uniform with the rest of the file (and
    /// with `buffer::Binding`). The reference's asymmetry looks incidental.
    pub fn unbind(self) {
        // SAFETY: binding name 0 to a valid target always succeeds.
        glad::context(|gl| unsafe { gl.BindTexture(self.target as GLenum, 0) });
    }

    /// PORTS: `Texture.zig:134-136`.
    ///
    /// ```zig
    /// pub fn generateMipmap(b: Binding) void {
    ///     glad.context.GenerateMipmap.?(@intFromEnum(b.target));
    /// }
    /// ```
    ///
    /// PORT NOTE: returns `void`, NOT an error — the reference does not collect
    /// `getError` here, so neither does this. Ported as-is rather than "improved".
    pub fn generate_mipmap(self) {
        // SAFETY: acts on the texture currently bound to `self.target`. Every
        // failure mode (incomplete texture, bad target) is a defined GL error the
        // reference deliberately does not read.
        glad::context(|gl| unsafe { gl.GenerateMipmap(self.target as GLenum) });
    }

    /// PORTS: `Texture.zig:147-166` (`adf161473`).
    ///
    /// ```zig
    /// pub fn parameter(b: Binding, comptime name: Parameter, value: name.Type()) errors.Error!void {
    ///     switch (name.Type()) {
    ///         c.GLint => glad.context.TexParameteri.?(
    ///             @intFromEnum(b.target),
    ///             @intFromEnum(name),
    ///             value,
    ///         ),
    ///         else => switch (@typeInfo(name.Type())) {
    ///             .@"enum" => glad.context.TexParameteri.?(
    ///                 @intFromEnum(b.target),
    ///                 @intFromEnum(name),
    ///                 @intFromEnum(value),
    ///             ),
    ///             else => @compileError("unknown parameter type"),
    ///         },
    ///     }
    ///     try errors.getError();
    /// }
    /// ```
    ///
    /// PORT NOTE (`value: name.Type()`): see [`Parameter::type_`]. The
    /// `@compileError` arm needs no port: [`ParameterValue`] is implemented only
    /// for `GLint` and the three enums. The comptime check that `value`'s type is
    /// `name.Type()` is an `assert!` here (R7).
    pub fn parameter<V: ParameterValue>(
        self,
        name: Parameter,
        value: V,
    ) -> Result<(), errors::Error> {
        assert!(
            V::TYPE == name.type_(),
            "texture parameter {name:?} takes {:?}, not {:?}",
            name.type_(),
            V::TYPE
        );
        let value = value.to_glint();
        // SAFETY: acts on the texture currently bound to `self.target`. A bad
        // name/value pairing is GL_INVALID_ENUM/GL_INVALID_VALUE, collected below.
        glad::context(|gl| unsafe {
            gl.TexParameteri(self.target as GLenum, name as GLenum, value)
        });
        errors::get_error()
    }

    /// PORTS: `Texture.zig:150-172`.
    ///
    /// ```zig
    /// pub fn image2D(
    ///     b: Binding,
    ///     level: c.GLint,
    ///     internal_format: InternalFormat,
    ///     width: c.GLsizei,
    ///     height: c.GLsizei,
    ///     format: Format,
    ///     typ: DataType,
    ///     data: ?*const anyopaque,
    /// ) errors.Error!void {
    ///     glad.context.TexImage2D.?(
    ///         @intFromEnum(b.target),
    ///         level,
    ///         @intFromEnum(internal_format),
    ///         width,
    ///         height,
    ///         0,
    ///         @intFromEnum(format),
    ///         @intFromEnum(typ),
    ///         data,
    ///     );
    ///     try errors.getError();
    /// }
    /// ```
    ///
    /// Note the hard-coded `0` — the `border` argument, which GL 3.0+ requires to
    /// be zero. It is the reference's, not an omission here.
    ///
    /// # Safety
    ///
    /// PORT NOTE (why `unsafe`, when the reference's fn is not): `data` is a raw
    /// pointer that the DRIVER dereferences, and how many bytes it reads is
    /// implied by `width`/`height`/`format`/`typ` plus the current
    /// `GL_UNPACK_*` pixel-store state — nothing in the signature. Safe Rust can
    /// forge a raw pointer (`4 as *const c_void`) without an `unsafe` block, so a
    /// SAFE fn here would let safe code cause UB, which is precisely what `unsafe
    /// fn` exists to prevent. Zig has no such distinction to port, so this is a
    /// soundness obligation Rust adds rather than a behaviour change.
    ///
    /// Callers must guarantee that `data` is either null or points to at least as
    /// many initialized bytes as `width * height * <bytes per texel of (format,
    /// typ)>`, adjusted for the current unpack alignment/row-length state.
    #[allow(clippy::too_many_arguments)]
    pub unsafe fn image_2d(
        self,
        level: GLint,
        internal_format: InternalFormat,
        width: GLsizei,
        height: GLsizei,
        format: Format,
        typ: DataType,
        data: *const c_void,
    ) -> Result<(), errors::Error> {
        // SAFETY: `data`'s validity is the caller's obligation, per the doc above.
        // GL copies out of it before returning — it retains nothing. A null `data`
        // is explicitly legal and allocates uninitialized storage.
        glad::context(|gl| unsafe {
            gl.TexImage2D(
                self.target as GLenum,
                level,
                internal_format.0,
                width,
                height,
                0,
                format.0,
                typ.0,
                data,
            );
        });
        errors::get_error()
    }

    /// PORTS: `Texture.zig:174-197`.
    ///
    /// ```zig
    /// pub fn subImage2D(
    ///     b: Binding,
    ///     level: c.GLint,
    ///     xoffset: c.GLint,
    ///     yoffset: c.GLint,
    ///     width: c.GLsizei,
    ///     height: c.GLsizei,
    ///     format: Format,
    ///     typ: DataType,
    ///     data: ?*const anyopaque,
    /// ) errors.Error!void {
    ///     glad.context.TexSubImage2D.?(
    ///         @intFromEnum(b.target),
    ///         level,
    ///         xoffset,
    ///         yoffset,
    ///         width,
    ///         height,
    ///         @intFromEnum(format),
    ///         @intFromEnum(typ),
    ///         data,
    ///     );
    ///     try errors.getError();
    /// }
    /// ```
    ///
    /// # Safety
    ///
    /// Same obligation as [`Binding::image_2d`] — see its note.
    #[allow(clippy::too_many_arguments)]
    pub unsafe fn sub_image_2d(
        self,
        level: GLint,
        xoffset: GLint,
        yoffset: GLint,
        width: GLsizei,
        height: GLsizei,
        format: Format,
        typ: DataType,
        data: *const c_void,
    ) -> Result<(), errors::Error> {
        // SAFETY: see `image_2d`. An offset+size past the image's bounds is
        // GL_INVALID_VALUE, collected below — GL bounds-checks against the
        // dimensions it was given, it does not read past `data`.
        glad::context(|gl| unsafe {
            gl.TexSubImage2D(
                self.target as GLenum,
                level,
                xoffset,
                yoffset,
                width,
                height,
                format.0,
                typ.0,
                data,
            );
        });
        errors::get_error()
    }

    /// PORTS: `Texture.zig:199-220`.
    ///
    /// ```zig
    /// pub fn copySubImage2D(
    ///     b: Binding,
    ///     level: c.GLint,
    ///     xoffset: c.GLint,
    ///     yoffset: c.GLint,
    ///     x: c.GLint,
    ///     y: c.GLint,
    ///     width: c.GLsizei,
    ///     height: c.GLsizei,
    /// ) errors.Error!void {
    ///     glad.context.CopyTexSubImage2D.?(
    ///         @intFromEnum(b.target),
    ///         level,
    ///         xoffset,
    ///         yoffset,
    ///         x,
    ///         y,
    ///         width,
    ///         height,
    ///     );
    ///     try errors.getError();
    /// }
    /// ```
    ///
    /// PORT NOTE: SAFE, unlike its two `image2D` siblings — the source pixels come
    /// from the bound READ framebuffer, not from a caller pointer, so there is no
    /// raw-pointer obligation for an `unsafe fn` to carry.
    #[allow(clippy::too_many_arguments)]
    pub fn copy_sub_image_2d(
        self,
        level: GLint,
        xoffset: GLint,
        yoffset: GLint,
        x: GLint,
        y: GLint,
        width: GLsizei,
        height: GLsizei,
    ) -> Result<(), errors::Error> {
        // SAFETY: reads from the bound READ framebuffer into the bound texture.
        // Every failure mode — an incomplete read framebuffer, out-of-bounds
        // offsets, a feedback loop — is a defined GL error, collected below.
        glad::context(|gl| unsafe {
            gl.CopyTexSubImage2D(
                self.target as GLenum,
                level,
                xoffset,
                yoffset,
                x,
                y,
                width,
                height,
            );
        });
        errors::get_error()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Binding, DataType, Format, InternalFormat, MagFilter, MinFilter, Parameter, ParameterType,
        Target, Wrap,
    };

    /// `adf161473`'s `Parameter.Type()`, arm by arm.
    #[test]
    fn parameter_types_match_the_reference() {
        assert_eq!(Parameter::MinFilter.type_(), ParameterType::MinFilter);
        assert_eq!(Parameter::MagFilter.type_(), ParameterType::MagFilter);
        for wrap in [Parameter::WrapS, Parameter::WrapT, Parameter::WrapR] {
            assert_eq!(wrap.type_(), ParameterType::Wrap);
        }
        for other in [
            Parameter::BaseLevel,
            Parameter::CompareFunc,
            Parameter::CompareMode,
            Parameter::LodBias,
            Parameter::MinLod,
            Parameter::MaxLod,
            Parameter::MaxLevel,
            Parameter::SwizzleR,
            Parameter::SwizzleG,
            Parameter::SwizzleB,
            Parameter::SwizzleA,
        ] {
            assert_eq!(other.type_(), ParameterType::GLint);
        }
    }

    /// A value of the wrong type for its parameter is the reference's COMPILE
    /// error; here it is an `assert!` that fires before any GL call, so no GL
    /// context is needed.
    #[test]
    #[should_panic(expected = "texture parameter MinFilter takes MinFilter, not Wrap")]
    fn parameter_rejects_a_value_of_the_wrong_type() {
        let binding = Binding {
            target: Target::Texture2D,
        };
        let _ = binding.parameter(Parameter::MinFilter, Wrap::Repeat);
    }

    /// RUSTTY-ONLY. The reference gets `enum(c_uint) { @"2D" = c.GL_TEXTURE_2D, … }`
    /// checked by the Zig compiler: the tag values ARE the `c.GL_*` constants, by
    /// construction. The Rust port re-states every one of them through an `as`
    /// cast or a newtype constant — and, for [`Target`], through a RENAMED variant
    /// — which is exactly where a typo or a mis-paired constant would hide and
    /// still compile. No GL context needed; these are compile-time constants.
    #[test]
    fn texture_enum_values_are_the_gl_constants() {
        assert_eq!(Target::Texture1D as u32, gl::TEXTURE_1D);
        assert_eq!(Target::Texture2D as u32, gl::TEXTURE_2D);
        assert_eq!(Target::Texture3D as u32, gl::TEXTURE_3D);
        assert_eq!(Target::Texture1DArray as u32, gl::TEXTURE_1D_ARRAY);
        assert_eq!(Target::Texture2DArray as u32, gl::TEXTURE_2D_ARRAY);
        assert_eq!(Target::Rectangle as u32, gl::TEXTURE_RECTANGLE);
        assert_eq!(Target::CubeMap as u32, gl::TEXTURE_CUBE_MAP);
        assert_eq!(Target::Buffer as u32, gl::TEXTURE_BUFFER);
        assert_eq!(
            Target::Texture2DMultisample as u32,
            gl::TEXTURE_2D_MULTISAMPLE
        );
        assert_eq!(
            Target::Texture2DMultisampleArray as u32,
            gl::TEXTURE_2D_MULTISAMPLE_ARRAY
        );

        assert_eq!(Parameter::BaseLevel as u32, gl::TEXTURE_BASE_LEVEL);
        assert_eq!(Parameter::CompareFunc as u32, gl::TEXTURE_COMPARE_FUNC);
        assert_eq!(Parameter::CompareMode as u32, gl::TEXTURE_COMPARE_MODE);
        assert_eq!(Parameter::LodBias as u32, gl::TEXTURE_LOD_BIAS);
        assert_eq!(Parameter::MinFilter as u32, gl::TEXTURE_MIN_FILTER);
        assert_eq!(Parameter::MagFilter as u32, gl::TEXTURE_MAG_FILTER);
        assert_eq!(Parameter::MinLod as u32, gl::TEXTURE_MIN_LOD);
        assert_eq!(Parameter::MaxLod as u32, gl::TEXTURE_MAX_LOD);
        assert_eq!(Parameter::MaxLevel as u32, gl::TEXTURE_MAX_LEVEL);
        assert_eq!(Parameter::SwizzleR as u32, gl::TEXTURE_SWIZZLE_R);
        assert_eq!(Parameter::SwizzleG as u32, gl::TEXTURE_SWIZZLE_G);
        assert_eq!(Parameter::SwizzleB as u32, gl::TEXTURE_SWIZZLE_B);
        assert_eq!(Parameter::SwizzleA as u32, gl::TEXTURE_SWIZZLE_A);
        assert_eq!(Parameter::WrapS as u32, gl::TEXTURE_WRAP_S);
        assert_eq!(Parameter::WrapT as u32, gl::TEXTURE_WRAP_T);
        assert_eq!(Parameter::WrapR as u32, gl::TEXTURE_WRAP_R);

        assert_eq!(MinFilter::Nearest as u32, gl::NEAREST);
        assert_eq!(MinFilter::Linear as u32, gl::LINEAR);
        assert_eq!(
            MinFilter::NearestMipmapNearest as u32,
            gl::NEAREST_MIPMAP_NEAREST
        );
        assert_eq!(
            MinFilter::LinearMipmapNearest as u32,
            gl::LINEAR_MIPMAP_NEAREST
        );
        assert_eq!(
            MinFilter::NearestMipmapLinear as u32,
            gl::NEAREST_MIPMAP_LINEAR
        );
        assert_eq!(
            MinFilter::LinearMipmapLinear as u32,
            gl::LINEAR_MIPMAP_LINEAR
        );

        assert_eq!(MagFilter::Nearest as u32, gl::NEAREST);
        assert_eq!(MagFilter::Linear as u32, gl::LINEAR);

        assert_eq!(Wrap::ClampToEdge as u32, gl::CLAMP_TO_EDGE);
        assert_eq!(Wrap::ClampToBorder as u32, gl::CLAMP_TO_BORDER);
        assert_eq!(Wrap::MirroredRepeat as u32, gl::MIRRORED_REPEAT);
        assert_eq!(Wrap::Repeat as u32, gl::REPEAT);

        assert_eq!(DataType::UNSIGNED_BYTE.0, gl::UNSIGNED_BYTE);
    }

    /// RUSTTY-ONLY. [`InternalFormat`] and [`Format`] DISAGREE on `rgb`/`rgba` in
    /// the reference — SIZED formats (`GL_RGB8`/`GL_RGBA8`) for the internal
    /// format, UNSIZED (`GL_RGB`/`GL_RGBA`) for the pixel format. That asymmetry
    /// reads like a typo and is the single most likely thing for a port (or a
    /// later "cleanup") to silently normalize, which would change what `image_2d`
    /// asks the driver to allocate. Pinned here so it cannot drift.
    #[test]
    fn internal_format_is_sized_where_format_is_unsized() {
        assert_eq!(InternalFormat::RED.0 as u32, gl::RED);
        assert_eq!(InternalFormat::RGB.0 as u32, gl::RGB8);
        assert_eq!(InternalFormat::RGBA.0 as u32, gl::RGBA8);
        assert_eq!(InternalFormat::SRGB.0 as u32, gl::SRGB8);
        assert_eq!(InternalFormat::SRGBA.0 as u32, gl::SRGB8_ALPHA8);
        assert_eq!(
            InternalFormat::RGBA_COMPRESSED.0 as u32,
            gl::COMPRESSED_RGBA_BPTC_UNORM
        );
        assert_eq!(
            InternalFormat::SRGBA_COMPRESSED.0 as u32,
            gl::COMPRESSED_SRGB_ALPHA_BPTC_UNORM
        );

        assert_eq!(Format::RED.0, gl::RED);
        assert_eq!(Format::RGB.0, gl::RGB);
        assert_eq!(Format::RGBA.0, gl::RGBA);
        assert_eq!(Format::BGRA.0, gl::BGRA);

        // The asymmetry itself, stated outright: same member name, different
        // constant, on purpose.
        assert_ne!(InternalFormat::RGB.0 as u32, Format::RGB.0);
        assert_ne!(InternalFormat::RGBA.0 as u32, Format::RGBA.0);
    }
}
