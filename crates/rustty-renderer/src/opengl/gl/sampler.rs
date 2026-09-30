//! PORTS: ghostty `pkg/opengl/Sampler.zig` (43 loc), in full.
//!
//! PORT NOTE (`glad.context.<Fn>.?(…)`): see `draw.rs`'s module note.
//!
//! PORT NOTE (by-value `self`): see `buffer.rs`'s module note — the reference's
//! methods take `v: Sampler` / `self: Sampler` by value.
//!
//! PORT NOTE (NO `Binding` type): alone among the bindable objects in this
//! package, `Sampler` has none — `bind` returns `void`, and `parameter` is a
//! method on the SAMPLER rather than on a binding, because `glSamplerParameteri`
//! takes the sampler name directly and needs no prior bind. That is the
//! reference's shape and it is ported as-is; inventing a `Binding` here for
//! symmetry with `Texture`/`Framebuffer` would add a type the reference has no
//! call site for.

use gl::types::GLuint;

use super::errors;
use super::glad;
use super::texture::{self, ParameterValue};

/// PORTS: `Sampler.zig:1-9` — `const Sampler = @This(); … id: c.GLuint`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Sampler {
    pub id: GLuint,
}

impl Sampler {
    /// Create a single sampler.
    //
    // PORTS: `Sampler.zig:11-17`.
    //
    // ```zig
    // pub fn create() errors.Error!Sampler {
    //     var id: c.GLuint = undefined;
    //     glad.context.GenSamplers.?(1, &id);
    //     try errors.getError();
    //     return .{ .id = id };
    // }
    // ```
    //
    // PORT NOTE (`var id: c.GLuint = undefined`): zero-initialized here; see
    // `texture::Texture::create`'s note.
    pub fn create() -> Result<Self, errors::Error> {
        let mut id: GLuint = 0;
        // SAFETY: `glGenSamplers` writes exactly `n` (= 1) GLuints through the
        // pointer; `&mut id` is one live, aligned GLuint.
        glad::context(|gl| unsafe { gl.GenSamplers(1, &mut id) });
        errors::get_error()?;
        Ok(Sampler { id })
    }

    /// glBindSampler
    //
    // PORTS: `Sampler.zig:19-23`.
    //
    // ```zig
    // pub fn bind(v: Sampler, index: c_uint) !void {
    //     glad.context.BindSampler.?(index, v.id);
    //     try errors.getError();
    // }
    // ```
    //
    // PORT NOTE (`index`): a texture UNIT ORDINAL, and — unlike
    // `Texture.active` two files over — the reference does NOT add `GL_TEXTURE0`
    // to it, because `glBindSampler` takes the ordinal directly. The asymmetry is
    // the GL API's, and it is carried over rather than harmonized.
    pub fn bind(self, index: GLuint) -> Result<(), errors::Error> {
        // SAFETY: binds a name this `Sampler` owns to a texture unit. An
        // out-of-range unit is GL_INVALID_VALUE, collected below.
        glad::context(|gl| unsafe { gl.BindSampler(index, self.id) });
        errors::get_error()
    }

    /// PORTS: `Sampler.zig:25-48` (`adf161473`, `590ab157f`).
    ///
    /// ```zig
    /// pub fn parameter(
    ///     self: Sampler,
    ///     comptime name: Texture.Parameter,
    ///     value: name.Type(),
    /// ) errors.Error!void {
    ///     const T = name.Type();
    ///
    ///     switch (T) {
    ///         c.GLint => glad.context.SamplerParameteri.?(
    ///             self.id,
    ///             @intFromEnum(name),
    ///             value,
    ///         ),
    ///         else => switch (@typeInfo(T)) {
    ///             .@"enum" => glad.context.SamplerParameteri.?(
    ///                 self.id,
    ///                 @intFromEnum(name),
    ///                 @intFromEnum(value),
    ///             ),
    ///             else => @compileLog("unsupported parameter type", T),
    ///         },
    ///     }
    ///     try errors.getError();
    /// }
    /// ```
    ///
    /// PORT NOTE (`name: Texture.Parameter`): the reference SHARES one `Parameter`
    /// enum between `Texture.Binding.parameter` and this — hence the import of
    /// [`texture::Parameter`] rather than a sampler-local copy.
    ///
    /// PORT NOTE (`value: name.Type()`): see `texture::Binding::parameter`. The
    /// `@compileLog` arm needs no port ([`ParameterValue`] covers the only
    /// types), and the comptime type check is an `assert!` (R7).
    pub fn parameter<V: ParameterValue>(
        self,
        name: texture::Parameter,
        value: V,
    ) -> Result<(), errors::Error> {
        assert!(
            V::TYPE == name.type_(),
            "sampler parameter {name:?} takes {:?}, not {:?}",
            name.type_(),
            V::TYPE
        );
        let value = value.to_glint();
        // SAFETY: acts on a name this `Sampler` owns. A bad name/value pairing is
        // GL_INVALID_ENUM/GL_INVALID_VALUE, collected below.
        glad::context(|gl| unsafe {
            gl.SamplerParameteri(self.id, name as gl::types::GLenum, value)
        });
        errors::get_error()
    }

    /// PORTS: `Sampler.zig:41-43`.
    ///
    /// ```zig
    /// pub fn destroy(v: Sampler) void {
    ///     glad.context.DeleteSamplers.?(1, &v.id);
    /// }
    /// ```
    ///
    /// PORT NOTE: by-value `self`, NOT a `Drop` impl — see
    /// `buffer::Buffer::destroy`'s note.
    pub fn destroy(self) {
        // SAFETY: `glDeleteSamplers` reads exactly `n` (= 1) GLuints through the
        // pointer; `&self.id` is one live, aligned GLuint. Deleting an already-
        // deleted or unknown name is silently ignored by GL, not UB.
        glad::context(|gl| unsafe { gl.DeleteSamplers(1, &self.id) });
    }
}
